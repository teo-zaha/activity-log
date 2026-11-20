use anyhow::{Result, anyhow};

use crate::platform::system_log_access;

/// Whether the System event log can be read; a refusal keeps its typed cause.
pub async fn check_system_support() -> Result<()> {
    tokio::task::spawn_blocking(|| system_log_access("System"))
        .await
        .map_err(|err| anyhow!("Reading the event log stopped unexpectedly: {err}"))?
}
