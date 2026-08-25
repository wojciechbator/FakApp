//! Declarative config, loaded from one JSON file. Secrets (SMTP password,
//! Discord webhook URL) come from the environment so the config file can live
//! in a repo.

use std::path::Path;

use serde::Deserialize;

const MIN_SECRET_LENGTH: usize = 8;
const DISCORD_WEBHOOK_PREFIXES: [&str; 2] = [
    "https://discord.com/api/webhooks/",
    "https://discordapp.com/api/webhooks/",
];
/// Upper bound on the probe interval (one hour). Keeps `repeat_alert_minutes
/// * 60` arithmetic and forgotten configs sane.
const MAX_INTERVAL_SECS: u64 = 3600;
/// Upper bound on the reminder period (one year in minutes); with the floor
/// of 1 this keeps `repeat_alert_minutes * 60` far from overflowing.
const MAX_REPEAT_ALERT_MINUTES: u64 = 525_600;
const MAX_TITLE_CHARS: usize = 128;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Where the dashboard listens. Loopback by default; put Caddy in front.
    #[serde(default = "default_listen")]
    pub listen: String,
    #[serde(default = "default_state_file")]
    pub state_file: String,
    /// Seconds between probes of each target.
    #[serde(default = "default_interval")]
    pub interval_secs: u64,
    /// Header on every alert ("Ups, FAKAP!"). The tool manager owns the tone.
    #[serde(default = "default_alert_title")]
    pub alert_title: String,
    /// Header on the all-clear ("Nie ma fakapu").
    #[serde(default = "default_recovery_title")]
    pub recovery_title: String,
    #[serde(default)]
    pub smtp: Option<Smtp>,
    /// Present (even empty) means Discord alerts are enabled; the webhook URL
    /// itself is read from `FAKAP_DISCORD_WEBHOOK_URL`.
    #[serde(default)]
    pub discord: Option<Discord>,
    pub targets: Vec<Target>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Smtp {
    pub host: String,
    #[serde(default = "default_smtp_port")]
    pub port: u16,
    pub user: String,
    /// Read from `FAKAP_SMTP_PASSWORD`; never stored in the config file.
    #[serde(skip)]
    pub password: String,
    pub from: String,
    pub to: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Discord {
    /// Read from `FAKAP_DISCORD_WEBHOOK_URL`; never stored in the config file.
    #[serde(skip)]
    pub webhook_url: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    /// Stable identifier used in state, logs and alert details.
    pub id: String,
    /// Human label shown on the dashboard and in alerts.
    pub name: String,
    /// Probed with GET; any status outside `expect` counts as a failure.
    pub url: String,
    /// HTTP statuses treated as healthy.
    #[serde(default = "default_expect")]
    pub expect: Vec<u16>,
    /// Consecutive failures before the monitor declares DOWN.
    #[serde(default = "default_failures")]
    pub failures_to_down: u32,
    /// Consecutive successes before it declares UP again.
    #[serde(default = "default_successes")]
    pub successes_to_up: u32,
    /// While DOWN, re-send the alert after this many minutes of silence.
    #[serde(default = "default_repeat_minutes")]
    pub repeat_alert_minutes: u64,
}

fn default_listen() -> String {
    "127.0.0.1:8183".to_owned()
}
fn default_state_file() -> String {
    "fakap-state.json".to_owned()
}
fn default_interval() -> u64 {
    30
}
fn default_alert_title() -> String {
    "Ups, FAKAP!".to_owned()
}
fn default_recovery_title() -> String {
    "Nie ma fakapu".to_owned()
}
fn default_smtp_port() -> u16 {
    587
}
fn default_expect() -> Vec<u16> {
    vec![200]
}
fn default_failures() -> u32 {
    3
}
fn default_successes() -> u32 {
    2
}
fn default_repeat_minutes() -> u64 {
    30
}

impl Config {
    pub fn load(path: &str) -> anyhow::Result<Config> {
        let raw = std::fs::read_to_string(Path::new(path))?;
        let mut config: Config = serde_json::from_str(&raw)?;
        if let Some(smtp) = &mut config.smtp {
            smtp.password = std::env::var("FAKAP_SMTP_PASSWORD").unwrap_or_default();
            validate_smtp(smtp)?;
        }
        if let Some(discord) = &mut config.discord {
            discord.webhook_url = std::env::var("FAKAP_DISCORD_WEBHOOK_URL").unwrap_or_default();
            validate_discord(discord)?;
        }
        validate_targets(&config.targets)?;
        anyhow::ensure!(
            config.interval_secs >= 5,
            "interval_secs must be at least 5"
        );
        anyhow::ensure!(
            config.interval_secs <= MAX_INTERVAL_SECS,
            "interval_secs must be at most {MAX_INTERVAL_SECS}"
        );
        anyhow::ensure!(
            !config.alert_title.trim().is_empty()
                && config.alert_title.chars().count() <= MAX_TITLE_CHARS,
            "alert_title must be 1..={MAX_TITLE_CHARS} characters"
        );
        anyhow::ensure!(
            !config.recovery_title.trim().is_empty()
                && config.recovery_title.chars().count() <= MAX_TITLE_CHARS,
            "recovery_title must be 1..={MAX_TITLE_CHARS} characters"
        );
        // A watchdog with no way to speak is decorative; at least one
        // channel (Discord or SMTP) must be configured.
        anyhow::ensure!(
            config.smtp.is_some() || config.discord.is_some(),
            "configure at least one notifier: discord or smtp"
        );
        Ok(config)
    }

    pub fn mailer(&self) -> Option<&Smtp> {
        self.smtp.as_ref()
    }

    pub fn discord(&self) -> Option<&Discord> {
        self.discord.as_ref()
    }
}

fn validate_smtp(smtp: &Smtp) -> anyhow::Result<()> {
    anyhow::ensure!(!smtp.host.trim().is_empty(), "smtp.host must be set");
    anyhow::ensure!(
        smtp.password.len() >= MIN_SECRET_LENGTH,
        "FAKAP_SMTP_PASSWORD must be at least {MIN_SECRET_LENGTH} characters"
    );
    anyhow::ensure!(
        !smtp.to.is_empty(),
        "smtp.to must list at least one recipient"
    );
    Ok(())
}

fn validate_discord(discord: &Discord) -> anyhow::Result<()> {
    let url = &discord.webhook_url;
    anyhow::ensure!(
        !url.is_empty(),
        "FAKAP_DISCORD_WEBHOOK_URL must be set when the discord notifier is configured"
    );
    anyhow::ensure!(
        url.len() >= MIN_SECRET_LENGTH
            && DISCORD_WEBHOOK_PREFIXES
                .iter()
                .any(|prefix| url.starts_with(prefix)),
        "FAKAP_DISCORD_WEBHOOK_URL must look like https://discord.com/api/webhooks/<id>/<token>"
    );
    Ok(())
}

fn validate_targets(targets: &[Target]) -> anyhow::Result<()> {
    anyhow::ensure!(!targets.is_empty(), "at least one target is required");
    let mut seen = std::collections::HashSet::new();
    for target in targets {
        anyhow::ensure!(
            !target.id.is_empty()
                && target.id.len() <= 64
                && target
                    .id
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'),
            "target id {:?} must be lowercase ascii, digits or '-' (max 64)",
            target.id
        );
        anyhow::ensure!(
            seen.insert(target.id.clone()),
            "duplicate target id {}",
            target.id
        );
        let parsed = url::Url::parse(&target.url).map_err(|error| {
            anyhow::anyhow!(
                "target {}: invalid url {:?}: {}",
                target.id,
                target.url,
                error
            )
        })?;
        anyhow::ensure!(
            matches!(parsed.scheme(), "https" | "http"),
            "target {}: url must be http(s)",
            target.id
        );
        anyhow::ensure!(
            !parsed.host_str().unwrap_or_default().is_empty(),
            "target {}: url has no host",
            target.id
        );
        anyhow::ensure!(
            target.failures_to_down >= 1 && target.successes_to_up >= 1,
            "target {}: thresholds must be at least 1",
            target.id
        );
        anyhow::ensure!(
            target
                .expect
                .iter()
                .all(|status| (100..=599).contains(status)),
            "target {}: expect must list HTTP statuses 100-599",
            target.id
        );
        anyhow::ensure!(
            (1..=MAX_REPEAT_ALERT_MINUTES).contains(&target.repeat_alert_minutes),
            "target {}: repeat_alert_minutes must be 1..={MAX_REPEAT_ALERT_MINUTES}",
            target.id
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const WEBHOOK: &str =
        "https://discord.com/api/webhooks/1234567890/aBcDeFgHiJkLmNoPqRsTuVwXyZ-0987654321";
    const BASE: &str = r#"{
        "discord": {},
        "targets": [{"id": "crowdrelay", "name": "CrowdRelay",
                     "url": "https://signal-api.virya.music/health/ready"}]
    }"#;

    fn parse(json: &str) -> Result<Config, anyhow::Error> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        // Tests run in parallel inside one process; a per-call name keeps
        // them from overwriting each other's fixture.
        let unique = AtomicU64::fetch_add(&COUNTER, 1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("fakap-test-{}-{unique}.json", std::process::id()));
        std::fs::write(&path, json).unwrap();
        let result = Config::load(path.to_str().unwrap());
        let _ = std::fs::remove_file(path);
        result
    }

    // One sequential test: every case mutates process-global env, so parallel
    // cases would race each other.
    #[test]
    fn config_validation_rules() {
        unsafe { std::env::set_var("FAKAP_DISCORD_WEBHOOK_URL", WEBHOOK) };
        let config = parse(BASE).expect("valid config");
        assert_eq!(config.targets.len(), 1);
        assert_eq!(config.targets[0].expect, vec![200]);
        assert_eq!(config.targets[0].failures_to_down, 3);
        assert_eq!(config.alert_title, "Ups, FAKAP!");
        assert_eq!(config.recovery_title, "Nie ma fakapu");
        assert_eq!(config.discord().unwrap().webhook_url, WEBHOOK);

        // The alert header belongs to the tool manager.
        let titled = r#"{"alert_title":"Fakap mówi:","discord":{},"targets":[{"id":"a","name":"a","url":"https://a.example"}]}"#;
        unsafe { std::env::set_var("FAKAP_DISCORD_WEBHOOK_URL", WEBHOOK) };
        assert_eq!(parse(titled).expect("titled").alert_title, "Fakap mówi:");

        unsafe { std::env::remove_var("FAKAP_DISCORD_WEBHOOK_URL") };
        assert!(parse(BASE).is_err(), "missing webhook url refused");

        unsafe { std::env::set_var("FAKAP_DISCORD_WEBHOOK_URL", "https://evil.example/hook") };
        assert!(parse(BASE).is_err(), "non-discord webhook refused");

        // No notifier at all: refused even though targets are fine.
        let silent = r#"{"targets":[{"id":"a","name":"a","url":"https://a.example"}]}"#;
        unsafe { std::env::remove_var("FAKAP_DISCORD_WEBHOOK_URL") };
        assert!(parse(silent).is_err(), "notifier-less config refused");

        unsafe { std::env::set_var("FAKAP_DISCORD_WEBHOOK_URL", WEBHOOK) };
        assert!(parse(r#"{"targets":[]}"#).is_err(), "empty targets refused");
        let dup = r#"{"discord":{},"targets":[{"id":"same","name":"a","url":"https://a.example"},{"id":"same","name":"b","url":"https://b.example"}]}"#;
        assert!(parse(dup).is_err(), "duplicate ids refused");
        let bad_url = r#"{"discord":{},"targets":[{"id":"x","name":"x","url":"ftp://nope"}]}"#;
        assert!(parse(bad_url).is_err(), "non-http url refused");

        let slow = r#"{"interval_secs":3601,"discord":{},"targets":[{"id":"a","name":"a","url":"https://a.example"}]}"#;
        assert!(parse(slow).is_err(), "over-long interval refused");
        let eager = r#"{"discord":{},"targets":[{"id":"a","name":"a","url":"https://a.example","repeat_alert_minutes":0}]}"#;
        assert!(parse(eager).is_err(), "zero repeat window refused");
        let absurd_repeat = r#"{"discord":{},"targets":[{"id":"a","name":"a","url":"https://a.example","repeat_alert_minutes":525601}]}"#;
        assert!(
            parse(absurd_repeat).is_err(),
            "over-long repeat window refused"
        );
        let bad_status = r#"{"discord":{},"targets":[{"id":"a","name":"a","url":"https://a.example","expect":[200,999]}]}"#;
        assert!(
            parse(bad_status).is_err(),
            "out-of-range expected status refused"
        );
        let low_status = r#"{"discord":{},"targets":[{"id":"a","name":"a","url":"https://a.example","expect":[42]}]}"#;
        assert!(
            parse(low_status).is_err(),
            "sub-100 expected status refused"
        );
    }
}
