//! The probe loop. One task per target: probe, record, and — when the state
//! machine says so — shout at the configured channels.

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use crate::Shared;
use crate::discord::{self, Level};
use crate::state::Outcome;

pub struct Checker {
    pub config: crate::Config,
    pub state: Shared,
    pub target_id: String,
    /// Shared probe/alert client, built once at startup.
    pub client: reqwest::Client,
    /// Shared SMTP transport, built once at startup.
    pub mailer: Option<Arc<crate::mailer::Mailer>>,
}

enum Probe {
    Ok(u64),
    Failed(String),
}

impl Checker {
    pub async fn run(self) {
        let Some(target) = self.config.targets.iter().find(|t| t.id == self.target_id) else {
            return;
        };
        let interval = Duration::from_secs(self.config.interval_secs);
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            ticker.tick().await;
            let probe = probe(&self.client, &target.url, &target.expect).await;
            let outcome = {
                let mut shared = self.state.lock().await;
                let Some(mut monitor) = shared.monitor(&self.config, &self.target_id) else {
                    continue;
                };
                let outcome = match probe {
                    Probe::Ok(latency_ms) => {
                        monitor.record(SystemTime::now(), true, Some(latency_ms), None)
                    }
                    Probe::Failed(error) => {
                        tracing::warn!(target = %self.target_id, %error, "probe failed");
                        monitor.record(SystemTime::now(), false, None, Some(error))
                    }
                };
                shared.write_back(&self.target_id, &monitor);
                outcome
            };
            if !matches!(outcome, Outcome::Quiet) && self.notify(outcome).await {
                // The page actually went out: only now does the reminder
                // clock start. A failed delivery leaves `last_alert` unset,
                // so the next probe retries immediately instead of after the
                // whole repeat window.
                let mut shared = self.state.lock().await;
                if let Some(mut monitor) = shared.monitor(&self.config, &self.target_id) {
                    monitor.mark_alerted(SystemTime::now());
                    shared.write_back(&self.target_id, &monitor);
                }
            }
        }
    }

    /// Fans one alert out to every configured channel. A failing channel is
    /// logged and never blocks the other one — partial delivery beats none.
    /// Answers whether at least one channel got the message through.
    async fn notify(&self, outcome: Outcome) -> bool {
        if matches!(outcome, Outcome::Quiet) {
            return false;
        }
        let target_name = self
            .config
            .targets
            .iter()
            .find(|t| t.id == self.target_id)
            .map(|t| t.name.as_str())
            .unwrap_or(self.target_id.as_str());
        // Quiet was filtered by the caller; the catch-all keeps this match
        // total without an unreachable arm that could only ever panic.
        let level = match outcome {
            Outcome::Recovered => Level::Recovered,
            Outcome::Remind => Level::Remind,
            Outcome::Down | Outcome::Quiet => Level::Down,
        };

        let description = format!(
            "{}\nservice: {}\nobserved at: {}",
            level.verdict_line(target_name),
            self.target_id,
            rfc3339_now(),
        );
        // Red channels keep the alarm header; the all-clear gets its own
        // friendlier one. Both are operator-owned config.
        let title = match outcome {
            Outcome::Recovered => self.config.recovery_title.clone(),
            _ => self.config.alert_title.clone(),
        };

        let mut delivered = false;
        if let Some(discord_config) = self.config.discord() {
            if let Err(error) = discord::send(
                &self.client,
                &discord_config.webhook_url,
                &title,
                &description,
                level,
            )
            .await
            {
                tracing::warn!(%error, target = %self.target_id, "discord alert not delivered");
            } else {
                delivered = true;
                tracing::info!(target = %self.target_id, "discord alert sent");
            }
        }

        if let Some(mailer) = &self.mailer {
            let subject = format!("{title} {target_name}");
            if let Err(error) = mailer.send(&subject, &description).await {
                tracing::warn!(%error, target = %self.target_id, "mail alert not delivered");
            } else {
                delivered = true;
            }
        }
        delivered
    }
}

fn rfc3339_now() -> String {
    time::OffsetDateTime::from(SystemTime::now())
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

async fn probe(client: &reqwest::Client, url: &str, expect: &[u16]) -> Probe {
    let started = SystemTime::now();
    let response = match client.get(url).send().await {
        Ok(response) => response,
        Err(error) => return Probe::Failed(format!("{url}: {error}")),
    };
    let latency = SystemTime::now()
        .duration_since(started)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0);
    let status = response.status().as_u16();
    if expect.contains(&status) {
        // Drain the body so the connection returns to the pool instead of dying.
        let _ = response.bytes().await;
        return Probe::Ok(latency);
    }
    // Drain the body so the connection returns to the pool instead of dying.
    let _ = response.bytes().await;
    Probe::Failed(format!("{url}: HTTP {status}, expected one of {expect:?}"))
}
