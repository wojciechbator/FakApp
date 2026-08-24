//! JSON persistence. The watchdog restarts on a box reboot; the state file is
//! what keeps that from re-paging an operator about an outage it already
//! announced — or staying silent about one that started while it was down.

use std::path::Path;

use anyhow::Context;

use crate::state::MonitorState;

pub fn load(path: &str) -> anyhow::Result<Option<MonitorState>> {
    let path = Path::new(path);
    if !path.exists() {
        return Ok(None);
    }
    let raw =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let state =
        serde_json::from_str(&raw).with_context(|| format!("parsing {}", path.display()))?;
    Ok(Some(state))
}

/// Written atomically (temp file + rename) so a crash mid-write cannot leave
/// half a JSON file behind for the next boot to choke on.
pub fn save(path: &str, state: &MonitorState) -> anyhow::Result<()> {
    let path = Path::new(path);
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, serde_json::to_vec(state)?)?;
    std::fs::rename(&temporary, path)?;
    Ok(())
}

/// Periodic checkpoint, independent of probe outcomes.
pub async fn saver(shared: crate::Shared, path: String) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        interval.tick().await;
        let snapshot = shared.lock().await.clone();
        if let Err(error) = save(&path, &snapshot) {
            tracing::warn!(%error, "failed to persist fakap state");
        }
    }
}
