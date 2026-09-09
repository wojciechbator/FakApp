//! Graduated remediation ladder. When a target transitions to DOWN, the
//! checker spawns a remediation task that tries increasingly aggressive
//! fixes over SSH, from cheapest to most disruptive:
//!
//! 1. Check if a GitHub Actions deploy is in progress — if so, do nothing.
//! 2. Restart the unhealthy container.
//! 3. `docker compose up` with the current tag (recreate from env).
//! 4. Full redeploy via the canonical deploy script.
//! 5. Notify humans — all rungs exhausted.
//!
//! Each rung is tried once per DOWN episode, with a cooldown between rungs.
//! The counter resets when the target recovers to UP. State is persisted in
//! the existing JSON state file so a watchdog restart doesn't retry rungs it
//! already attempted.

use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::Shared;
use crate::config::Config;
use crate::discord::{self, Level};

/// Remediation config, nested under each target that should auto-recover.
/// Targets without this section stay observe-only.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Remediation {
    /// SSH host alias from ~/.ssh/config (e.g. "virya-crowdrelay").
    pub ssh_host: String,
    /// Maximum remediation attempts per DOWN episode before paging humans.
    #[serde(default = "default_max_attempts")]
    pub max_attempts: u32,
    /// Seconds to wait between remediation rungs.
    #[serde(default = "default_cooldown")]
    pub cooldown_secs: u64,
    /// Seconds to wait after a remediation action before re-probing.
    #[serde(default = "default_settle_secs")]
    pub settle_secs: u64,
    /// Command to check if a GH Actions deploy is in progress.
    /// Output "1" or non-empty = deploy running.
    #[serde(default = "default_check_deploy")]
    pub check_deploy_cmd: String,
    /// Rung 1: restart the unhealthy container.
    pub restart_cmd: String,
    /// Rung 2: compose up with the current tag.
    #[serde(default)]
    pub compose_up_cmd: Option<String>,
    /// Rung 3: full redeploy via canonical script.
    #[serde(default)]
    pub redeploy_cmd: Option<String>,
}

fn default_max_attempts() -> u32 {
    3
}
fn default_cooldown() -> u64 {
    60
}
fn default_settle_secs() -> u64 {
    30
}
fn default_check_deploy() -> String {
    "gh run list --repo CrowdRelay/crowdrelay --workflow deploy.yml --status in_progress --limit 1 --json databaseId --jq 'length'".to_owned()
}

/// Per-target remediation state, persisted in the JSON state file.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RemediationState {
    /// How many rungs have been attempted in this DOWN episode.
    #[serde(default)]
    pub attempts: u32,
    /// When the last rung was tried (to enforce cooldown).
    #[serde(default)]
    pub last_attempt_at: Option<SystemTime>,
    /// Which rung was last attempted (for logging).
    #[serde(default)]
    pub last_rung: Option<String>,
    /// Whether all rungs are exhausted and humans have been paged.
    #[serde(default)]
    pub exhausted: bool,
}

/// The result of one remediation attempt.
pub enum RemediationResult {
    /// A deploy is in progress — do nothing, the deploy will fix it.
    DeployInProgress,
    /// This rung succeeded — the target should recover.
    Fixed { rung: String },
    /// This rung failed — try the next one (after cooldown).
    Failed { rung: String, error: String },
    /// All rungs exhausted — page humans.
    Exhausted,
    /// Remediation not configured for this target.
    NotConfigured,
    /// Cooldown not yet elapsed — wait.
    CoolingDown { secs_remaining: u64 },
}

/// Runs the graduated remediation ladder for one target.
/// Called from the checker when a DOWN transition is detected.
///
/// This function reads and writes remediation state through the shared
/// state mutex, so concurrent probes of the same target don't double-fix.
pub async fn attempt(
    shared: &Shared,
    config: &Config,
    target_id: &str,
    http: &reqwest::Client,
    discord_webhook: Option<&str>,
    alert_title: &str,
) -> RemediationResult {
    let target = match config.targets.iter().find(|t| t.id == target_id) {
        Some(t) => t,
        None => return RemediationResult::NotConfigured,
    };
    let Some(remediation) = target.remediation.as_ref() else {
        return RemediationResult::NotConfigured;
    };

    // Read current remediation state under the lock.
    let (attempts, last_attempt_at, exhausted) = {
        let state = shared.lock().await;
        let Some(ts) = state.targets.get(target_id) else {
            return RemediationResult::NotConfigured;
        };
        let rs = ts.remediation.clone().unwrap_or_default();
        if rs.exhausted {
            return RemediationResult::Exhausted;
        }
        if rs.attempts >= remediation.max_attempts {
            return RemediationResult::Exhausted;
        }
        (rs.attempts, rs.last_attempt_at, rs.exhausted)
    };
    let _ = exhausted;

    // Cooldown check.
    if let Some(last) = last_attempt_at {
        let elapsed = SystemTime::now()
            .duration_since(last)
            .unwrap_or_default()
            .as_secs();
        if elapsed < remediation.cooldown_secs {
            return RemediationResult::CoolingDown {
                secs_remaining: remediation.cooldown_secs - elapsed,
            };
        }
    }

    // Rung 0: check if a deploy is in progress.
    if !remediation.check_deploy_cmd.is_empty() {
        match ssh_output(&remediation.ssh_host, &remediation.check_deploy_cmd).await {
            Ok(output) => {
                let trimmed = output.trim();
                if trimmed != "0" && !trimmed.is_empty() {
                    tracing::info!(
                        target = target_id,
                        "deploy in progress — skipping remediation"
                    );
                    record_attempt(shared, target_id, "deploy-in-progress", false).await;
                    return RemediationResult::DeployInProgress;
                }
            }
            Err(error) => {
                tracing::warn!(target = target_id, %error, "deploy check failed — proceeding with remediation");
            }
        }
    }

    let rung = pick_rung(attempts, remediation);
    let Some(cmd) = rung_command(&rung, remediation) else {
        // No more rungs available.
        mark_exhausted(shared, target_id).await;
        notify_exhausted(
            http,
            discord_webhook,
            alert_title,
            target_id,
            &target.name,
            "all remediation rungs attempted",
        )
        .await;
        return RemediationResult::Exhausted;
    };

    tracing::info!(target = target_id, rung = %rung, "attempting remediation rung");
    let result = ssh_output(&remediation.ssh_host, &cmd).await;

    // Record the attempt.
    let success = result.is_ok();
    record_attempt(shared, target_id, &rung, success).await;

    match result {
        Ok(_) => {
            // Wait for the service to settle, then let the next probe cycle
            // determine if the fix worked. The checker will detect UP and
            // reset the remediation counter.
            tokio::time::sleep(Duration::from_secs(remediation.settle_secs)).await;
            tracing::info!(target = target_id, rung = %rung, "remediation rung completed");
            RemediationResult::Fixed { rung }
        }
        Err(error) => {
            tracing::warn!(target = target_id, rung = %rung, %error, "remediation rung failed");
            // Check if we've exhausted all rungs.
            let next_attempts = attempts + 1;
            if next_attempts >= remediation.max_attempts {
                mark_exhausted(shared, target_id).await;
                notify_exhausted(
                    http,
                    discord_webhook,
                    alert_title,
                    target_id,
                    &target.name,
                    &format!("rung '{rung}' failed: {error}"),
                )
                .await;
                return RemediationResult::Exhausted;
            }
            RemediationResult::Failed {
                rung,
                error: error.to_string(),
            }
        }
    }
}

/// Picks which rung to try based on the attempt count.
fn pick_rung(attempts: u32, remediation: &Remediation) -> String {
    match attempts {
        0 => "restart".to_owned(),
        1 => {
            if remediation.compose_up_cmd.is_some() {
                "compose-up".to_owned()
            } else {
                "redeploy".to_owned()
            }
        }
        2 => {
            if remediation.redeploy_cmd.is_some() {
                "redeploy".to_owned()
            } else {
                "restart".to_owned()
            }
        }
        _ => "restart".to_owned(),
    }
}

/// Returns the SSH command for a given rung.
fn rung_command(rung: &str, remediation: &Remediation) -> Option<String> {
    match rung {
        "restart" => Some(remediation.restart_cmd.clone()),
        "compose-up" => remediation.compose_up_cmd.clone(),
        "redeploy" => remediation.redeploy_cmd.clone(),
        _ => None,
    }
}

/// Runs a command over SSH and returns its stdout.
/// Uses tokio::process::Command so it doesn't block the async runtime.
async fn ssh_output(host: &str, command: &str) -> anyhow::Result<String> {
    let output = tokio::process::Command::new("ssh")
        .arg(host)
        .arg(command)
        .output()
        .await
        .map_err(|error| anyhow::anyhow!("ssh to {host} failed: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(anyhow::anyhow!(
            "ssh command on {host} exited {}: stderr={stderr} stdout={stdout}",
            output.status
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Records a remediation attempt in the shared state.
async fn record_attempt(shared: &Shared, target_id: &str, rung: &str, _success: bool) {
    let mut state = shared.lock().await;
    if let Some(ts) = state.targets.get_mut(target_id) {
        let rs = ts.remediation.get_or_insert_default();
        rs.attempts += 1;
        rs.last_attempt_at = Some(SystemTime::now());
        rs.last_rung = Some(rung.to_owned());
    }
}

/// Marks remediation as exhausted for a target.
async fn mark_exhausted(shared: &Shared, target_id: &str) {
    let mut state = shared.lock().await;
    if let Some(ts) = state.targets.get_mut(target_id) {
        let rs = ts.remediation.get_or_insert_default();
        rs.exhausted = true;
    }
}

/// Resets remediation state when a target recovers to UP.
pub async fn reset(shared: &Shared, target_id: &str) {
    let mut state = shared.lock().await;
    if let Some(ts) = state.targets.get_mut(target_id) {
        if let Some(rs) = &mut ts.remediation {
            rs.attempts = 0;
            rs.last_attempt_at = None;
            rs.last_rung = None;
            rs.exhausted = false;
        }
    }
}

/// Sends a Discord alert when all remediation rungs are exhausted.
async fn notify_exhausted(
    http: &reqwest::Client,
    webhook_url: Option<&str>,
    alert_title: &str,
    target_id: &str,
    target_name: &str,
    detail: &str,
) {
    let Some(url) = webhook_url else {
        return;
    };
    let description = format!(
        "\u{1f6a8} **AUTO-REMEDIATION EXHAUSTED**\n\
         service: {target_id}\n\
         name: {target_name}\n\
         detail: {detail}\n\
         observed at: {}",
        rfc3339_now(),
    );
    if let Err(error) = discord::send(http, url, alert_title, &description, Level::Down).await {
        tracing::warn!(%error, target = target_id, "exhausted alert not delivered");
    }
}

fn rfc3339_now() -> String {
    time::OffsetDateTime::from(SystemTime::now())
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

/// Whether a target's DOWN outcome should trigger remediation.
pub fn should_remediate(config: &Config, target_id: &str) -> bool {
    config
        .targets
        .iter()
        .find(|t| t.id == target_id)
        .is_some_and(|t| t.remediation.is_some())
}

/// Whether a target's Recovered outcome should reset remediation state.
pub fn should_reset(config: &Config, target_id: &str) -> bool {
    should_remediate(config, target_id)
}

/// Spawns a remediation task for a DOWN transition.
/// Does not block the probe loop — runs in a detached tokio task.
pub fn spawn(
    shared: Shared,
    config: Config,
    target_id: String,
    http: reqwest::Client,
    discord_webhook: Option<String>,
    alert_title: String,
) {
    tokio::spawn(async move {
        let result = attempt(
            &shared,
            &config,
            &target_id,
            &http,
            discord_webhook.as_deref(),
            &alert_title,
        )
        .await;
        match result {
            RemediationResult::Fixed { rung } => {
                tracing::info!(target = %target_id, rung = %rung, "remediation fixed the target");
            }
            RemediationResult::DeployInProgress => {
                tracing::info!(target = %target_id, "remediation skipped — deploy in progress");
            }
            RemediationResult::Failed { rung, error } => {
                tracing::warn!(target = %target_id, rung = %rung, %error, "remediation rung failed — will retry after cooldown");
            }
            RemediationResult::Exhausted => {
                tracing::error!(target = %target_id, "remediation exhausted — humans notified");
            }
            RemediationResult::NotConfigured => {}
            RemediationResult::CoolingDown { secs_remaining } => {
                tracing::debug!(
                    target = %target_id,
                    secs_remaining,
                    "remediation cooling down"
                );
            }
        }
    });
}

/// Spawns a reset task for a Recovered transition.
pub fn spawn_reset(shared: Shared, config: Config, target_id: String) {
    if !should_reset(&config, &target_id) {
        return;
    }
    tokio::spawn(async move {
        reset(&shared, &target_id).await;
        tracing::info!(target = %target_id, "remediation state reset on recovery");
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick_rung_sequence() {
        let remediation = Remediation {
            ssh_host: "test".to_owned(),
            max_attempts: 3,
            cooldown_secs: 1,
            settle_secs: 1,
            check_deploy_cmd: "echo 0".to_owned(),
            restart_cmd: "echo restart".to_owned(),
            compose_up_cmd: Some("echo compose".to_owned()),
            redeploy_cmd: Some("echo redeploy".to_owned()),
        };
        assert_eq!(pick_rung(0, &remediation), "restart");
        assert_eq!(pick_rung(1, &remediation), "compose-up");
        assert_eq!(pick_rung(2, &remediation), "redeploy");
    }

    #[test]
    fn pick_rung_without_compose() {
        let remediation = Remediation {
            ssh_host: "test".to_owned(),
            max_attempts: 2,
            cooldown_secs: 1,
            settle_secs: 1,
            check_deploy_cmd: "echo 0".to_owned(),
            restart_cmd: "echo restart".to_owned(),
            compose_up_cmd: None,
            redeploy_cmd: Some("echo redeploy".to_owned()),
        };
        assert_eq!(pick_rung(0, &remediation), "restart");
        assert_eq!(pick_rung(1, &remediation), "redeploy");
    }

    #[test]
    fn rung_command_mapping() {
        let remediation = Remediation {
            ssh_host: "test".to_owned(),
            max_attempts: 3,
            cooldown_secs: 1,
            settle_secs: 1,
            check_deploy_cmd: String::new(),
            restart_cmd: "docker restart api".to_owned(),
            compose_up_cmd: Some("docker compose up".to_owned()),
            redeploy_cmd: Some("./deploy.sh".to_owned()),
        };
        assert_eq!(
            rung_command("restart", &remediation).as_deref(),
            Some("docker restart api")
        );
        assert_eq!(
            rung_command("compose-up", &remediation).as_deref(),
            Some("docker compose up")
        );
        assert_eq!(
            rung_command("redeploy", &remediation).as_deref(),
            Some("./deploy.sh")
        );
        assert!(rung_command("unknown", &remediation).is_none());
    }
}
