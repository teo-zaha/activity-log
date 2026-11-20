use crate::{
    platform::{ComGuard, outlook_is_installed},
    provider::source_failure::{SourceFailure, SourceProblem},
};

/// Only checks that Outlook is installed: talking to it would launch Outlook at app start.
/// Whether it is running is decided per collection.
pub async fn check_outlook_ready() -> anyhow::Result<()> {
    tokio::task::spawn_blocking(check_outlook_ready_blocking)
        .await
        .map_err(|err| anyhow::anyhow!("Outlook startup check task failed: {err}"))?
}

fn check_outlook_ready_blocking() -> anyhow::Result<()> {
    let _com = ComGuard::new()?;
    if outlook_is_installed() {
        Ok(())
    } else {
        Err(SourceFailure::new(SourceProblem::NotInstalled, "Outlook not installed").into())
    }
}
