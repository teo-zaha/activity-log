use tokio::process::Command;

use crate::provider::system::SystemSupport;

pub async fn check_system_support() -> SystemSupport {
    match check_last_access().await {
        Ok(()) => SystemSupport::Available,
        Err(err) => SystemSupport::Unavailable(format!("boot/shutdown history unavailable: {err}")),
    }
}

async fn check_last_access() -> Result<(), String> {
    let output = Command::new("/usr/bin/last")
        .arg("-n")
        .arg("1")
        .output()
        .await
        .map_err(|err| err.to_string())?;

    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}
