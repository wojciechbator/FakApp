//! Per-target status and the transition rules that decide when a human hears
//! about it. Pure logic lives here; I/O stays in `checker` and `mailer`, so
//! every rule below is unit-testable without a network.

use std::collections::VecDeque;
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::config::Config;

/// How many individual probe results are kept per target for the dashboard
/// strip and the uptime estimate. At a 30 s interval that is two hours.
pub const HISTORY_LEN: usize = 240;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Probe has never answered since state began. Reported like DOWN but
    /// alerted only once, so a dashboard restart during an outage still pages.
    Unknown,
    Up,
    Down,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Up => "up",
            Self::Down => "down",
        }
    }

    pub fn healthy(self) -> bool {
        matches!(self, Self::Up)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckRecord {
    pub at: SystemTime,
    pub ok: bool,
    /// Milliseconds for a successful probe; absent on transport errors.
    #[serde(default)]
    pub latency_ms: Option<u64>,
    /// Human-readable failure detail for the dashboard and alert mails.
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TargetState {
    pub status: Option<Status>,
    pub consecutive_failures: u32,
    pub consecutive_successes: u32,
    pub last_change: Option<SystemTime>,
    pub last_alert: Option<SystemTime>,
    pub history: VecDeque<CheckRecord>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// Nothing to tell anybody.
    #[default]
    Quiet,
    /// The monitor just flipped to UP — send the recovery mail.
    Recovered,
    /// The monitor just flipped to DOWN — send the outage mail.
    Down,
    /// Still down past `repeat_alert_minutes` — remind, do not spam.
    Remind,
}

pub struct TargetMonitor {
    pub inner: TargetState,
    failures_to_down: u32,
    successes_to_up: u32,
    repeat_alert: Duration,
}

impl TargetMonitor {
    pub fn from_target(target: &crate::config::Target, inner: TargetState) -> Self {
        Self {
            inner,
            failures_to_down: target.failures_to_down,
            successes_to_up: target.successes_to_up,
            repeat_alert: Duration::from_secs(target.repeat_alert_minutes * 60),
        }
    }

    /// Records one probe result and answers whether anybody must be told.
    ///
    /// The thresholds give hysteresis: one failed request over flaky Wi-Fi or
    /// one lucky success in the middle of a crash loop must not flip state,
    /// because every flip is a mail in somebody's pocket.
    pub fn record(
        &mut self,
        at: SystemTime,
        ok: bool,
        latency_ms: Option<u64>,
        error: Option<String>,
    ) -> Outcome {
        let previous = self.inner.status.unwrap_or(Status::Unknown);
        self.push_history(CheckRecord {
            at,
            ok,
            latency_ms,
            error,
        });

        if ok {
            self.inner.consecutive_successes += 1;
            self.inner.consecutive_failures = 0;
            if !previous.healthy() && self.inner.consecutive_successes >= self.successes_to_up {
                self.transition(at, Status::Up);
                return Outcome::Recovered;
            }
            return Outcome::Quiet;
        }

        self.inner.consecutive_failures += 1;
        self.inner.consecutive_successes = 0;
        if previous.healthy() || previous == Status::Unknown {
            if self.inner.consecutive_failures >= self.failures_to_down {
                self.transition(at, Status::Down);
                return Outcome::Down;
            }
            return Outcome::Quiet;
        }
        // Already DOWN: remind after the quiet period, never more often.
        let due = self
            .inner
            .last_alert
            .map(|sent| at.duration_since(sent).unwrap_or_default() >= self.repeat_alert)
            .unwrap_or(true);
        if due {
            self.inner.last_alert = Some(at);
            return Outcome::Remind;
        }
        Outcome::Quiet
    }

    fn transition(&mut self, at: SystemTime, status: Status) {
        self.inner.status = Some(status);
        self.inner.last_change = Some(at);
        // A fresh transition always alerts; the reminder clock starts now.
        self.inner.last_alert = Some(at);
    }

    fn push_history(&mut self, record: CheckRecord) {
        self.inner.history.push_back(record);
        while self.inner.history.len() > HISTORY_LEN {
            self.inner.history.pop_front();
        }
    }

    /// Uptime share across the retained history. `None` until there is data —
    /// reporting 100% before the first probe would be a lie by omission.
    pub fn uptime_ratio(&self) -> Option<f64> {
        let total = self.inner.history.len();
        if total == 0 {
            return None;
        }
        let ok = self.inner.history.iter().filter(|record| record.ok).count();
        Some(ok as f64 / total as f64)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MonitorState {
    /// Keyed by target id; targets removed from config are simply ignored.
    #[serde(flatten)]
    pub targets: std::collections::BTreeMap<String, TargetState>,
}

impl MonitorState {
    /// Restores prior state so a watchdog restart does not re-page on an
    /// outage it already announced (or stay silent about one it missed).
    pub fn restore(config: &Config, previous: Option<MonitorState>) -> MonitorState {
        let mut state = previous.unwrap_or_default();
        for target in &config.targets {
            let entry = state.targets.entry(target.id.clone()).or_default();
            if entry.status.is_none() {
                entry.status = Some(Status::Unknown);
            }
        }
        state
    }

    pub fn monitor(&self, config: &Config, target_id: &str) -> Option<TargetMonitor> {
        let target = config.targets.iter().find(|t| t.id == target_id)?;
        let inner = self.targets.get(target_id).cloned().unwrap_or_default();
        Some(TargetMonitor::from_target(target, inner))
    }

    pub fn write_back(&mut self, target_id: &str, monitor: &TargetMonitor) {
        self.targets
            .insert(target_id.to_owned(), monitor.inner.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Target;

    fn monitor(failures: u32, successes: u32, repeat_minutes: u64) -> TargetMonitor {
        TargetMonitor::from_target(
            &Target {
                id: "t".into(),
                name: "T".into(),
                url: "https://t.example/healthz".into(),
                expect: vec![200],
                failures_to_down: failures,
                successes_to_up: successes,
                repeat_alert_minutes: repeat_minutes,
            },
            TargetState::default(),
        )
    }

    fn seconds_since(epoch: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(epoch)
    }

    #[test]
    fn a_single_blip_is_not_an_outage() {
        let mut monitor = monitor(3, 2, 30);
        assert_eq!(
            monitor.record(seconds_since(10), true, Some(20), None),
            Outcome::Quiet
        );
        assert_eq!(
            monitor.record(seconds_since(40), false, None, Some("timeout".into())),
            Outcome::Quiet
        );
        assert_eq!(
            monitor.record(seconds_since(70), false, None, None),
            Outcome::Quiet
        );
        // Two of three failures: not yet DOWN.
        assert!(!matches!(monitor.inner.status, Some(Status::Down)));
    }

    #[test]
    fn consecutive_failures_declare_down_and_alert_once() {
        let mut monitor = monitor(3, 2, 30);
        assert_eq!(
            monitor.record(seconds_since(1), false, None, None),
            Outcome::Quiet
        );
        assert_eq!(
            monitor.record(seconds_since(2), false, None, None),
            Outcome::Quiet
        );
        assert_eq!(
            monitor.record(seconds_since(3), false, None, None),
            Outcome::Down
        );
        assert!(!monitor.inner.status.unwrap().healthy());
    }

    #[test]
    fn recovery_needs_a_run_of_successes() {
        let mut monitor = monitor(2, 3, 30);
        monitor.record(seconds_since(1), false, None, None);
        monitor.record(seconds_since(2), false, None, None); // DOWN
        assert_eq!(
            monitor.record(seconds_since(60), true, Some(15), None),
            Outcome::Quiet
        );
        assert_eq!(
            monitor.record(seconds_since(90), true, Some(12), None),
            Outcome::Quiet
        );
        assert_eq!(
            monitor.record(seconds_since(120), true, Some(11), None),
            Outcome::Recovered
        );
        assert!(monitor.inner.status.unwrap().healthy());
    }

    #[test]
    fn reminders_wait_for_the_repeat_window() {
        let mut monitor = monitor(1, 1, 30);
        monitor.record(seconds_since(0), false, None, None); // DOWN + first alert
        // Still inside the 30 minute quiet window.
        assert_eq!(
            monitor.record(seconds_since(29 * 60), false, None, None),
            Outcome::Quiet
        );
        // Past it: exactly one reminder.
        assert_eq!(
            monitor.record(seconds_since(31 * 60), false, None, None),
            Outcome::Remind
        );
        assert_eq!(
            monitor.record(seconds_since(35 * 60), false, None, None),
            Outcome::Quiet
        );
    }

    #[test]
    fn uptime_is_the_observed_share_and_none_before_any_data() {
        let mut monitor = monitor(1, 1, 30);
        assert_eq!(monitor.uptime_ratio(), None);
        monitor.record(seconds_since(1), true, Some(5), None);
        monitor.record(seconds_since(2), false, None, None);
        assert_eq!(monitor.uptime_ratio(), Some(0.5));
    }

    #[test]
    fn history_is_bounded_so_state_cannot_grow_forever() {
        let mut monitor = monitor(1, 1, 30);
        for second in 0..(HISTORY_LEN as u64 + 50) {
            monitor.record(seconds_since(second), true, Some(1), None);
        }
        assert_eq!(monitor.inner.history.len(), HISTORY_LEN);
    }
}
