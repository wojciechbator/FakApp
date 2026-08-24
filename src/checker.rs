//! The probe loop. One task per target: probe, record, and — when the state
//! machine says so — shout at the configured channels.

use std::time::{Duration, SystemTime};

use crate::Shared;
use crate::discord::{self, Level};
use crate::state::Outcome;

pub struct Checker {
    pub config: crate::Config,
    pub state: Shared,
    pub target_id: String,
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
        // One client for probes and alerts. A failed build means this checker
        // cannot ever probe: log loudly and drop the task, never panic.
        let client = match reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .user_agent(concat!("fakap/", env!("CARGO_PKG_VERSION")))
            .build()
        {
            Ok(client) => client,
            Err(error) => {
                tracing::error!(%error, target = %self.target_id, "probe client unavailable");
                return;
            }
        };
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            ticker.tick().await;
            let probe = probe(&client, &target.url, &target.expect).await;
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
            if !matches!(outcome, Outcome::Quiet) {
                self.notify(&client, outcome).await;
            }
        }
    }

    /// Fans one alert out to every configured channel. A failing channel is
    /// logged and never blocks the other one — partial delivery beats none.
    async fn notify(&self, client: &reqwest::Client, outcome: Outcome) {
        if matches!(outcome, Outcome::Quiet) {
            return;
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
        let title = self.config.alert_title.clone();

        if let Some(discord_config) = self.config.discord() {
            if let Err(error) = discord::send(
                client,
                &discord_config.webhook_url,
                &title,
                &description,
                level,
            )
            .await
            {
                tracing::warn!(%error, target = %self.target_id, "discord alert not delivered");
            } else {
                tracing::info!(target = %self.target_id, "discord alert sent");
            }
        }

        if let Some(smtp) = self.config.mailer() {
            match crate::mailer::Mailer::new(smtp) {
                Ok(mailer) => {
                    let subject = format!("{title} {target_name}");
                    if let Err(error) = mailer.send(&subject, &description).await {
                        tracing::warn!(%error, target = %self.target_id, "mail alert not delivered");
                    }
                }
                Err(error) => tracing::error!(%error, "mail transport unavailable"),
            }
        }
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
        return Probe::Ok(latency);
    }
    // Drain the body so the connection returns to the pool instead of dying.
    let _ = response.bytes().await;
    Probe::Failed(format!("{url}: HTTP {status}, expected one of {expect:?}"))
}
