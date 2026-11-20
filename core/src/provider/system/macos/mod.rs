use anyhow::{Context, Result, bail};
use tokio::process::Command;

/// Whether `last` can read the boot and shutdown history.
pub async fn check_system_support() -> Result<()> {
    let output = Command::new("/usr/bin/last")
        .arg("-n")
        .arg("1")
        .output()
        .await
        .context("boot/shutdown history unavailable")?;
    if !output.status.success() {
        bail!(
            "boot/shutdown history unavailable: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}
