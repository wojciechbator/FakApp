//! Discord webhook delivery. One POST per alert, embed-styled so the header
//! ("Ups, FAKAP!" — or whatever the tool manager set) reads at a glance.

use anyhow::Context;
use serde_json::json;

const GREEN: u32 = 0x57f287;
const RED: u32 = 0xed4245;

#[derive(Clone, Copy)]
pub enum Level {
    Down,
    Remind,
    Recovered,
}

impl Level {
    fn color(self) -> u32 {
        match self {
            Self::Recovered => GREEN,
            _ => RED,
        }
    }

    /// The first line of the details: verdict with the service name.
    pub fn verdict_line(self, name: &str) -> String {
        match self {
            Self::Down => format!("\u{1f534} **{name}** is DOWN"),
            Self::Remind => format!("\u{1f534} **{name}** is still DOWN"),
            Self::Recovered => format!("\u{1f7e9} **{name}** is UP again"),
        }
    }
}

/// Posts one embed. Discord answers 204 on success; anything else is an
/// error the checker logs and survives (the next reminder retries).
pub async fn send(
    client: &reqwest::Client,
    webhook_url: &str,
    title: &str,
    description: &str,
    level: Level,
) -> anyhow::Result<()> {
    let payload = json!({
        "embeds": [{
            "title": title,
            "description": description,
            "color": level.color(),
        }]
    });
    let response = client
        .post(webhook_url)
        .timeout(std::time::Duration::from_secs(10))
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(payload.to_string())
        .send()
        .await
        .context("discord webhook request failed")?;
    let status = response.status();
    // Drain so the connection returns to the pool.
    let _ = response.bytes().await;
    anyhow::ensure!(
        status.is_success(),
        "discord webhook returned HTTP {status}"
    );
    Ok(())
}
