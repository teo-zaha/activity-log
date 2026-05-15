use tokio::process::Command;

use super::SystemSupport;

pub async fn check_system_support() -> SystemSupport {
    match check_log_access("System").await {
        Ok(()) => SystemSupport::Available,
        Err(err) => SystemSupport::Unavailable(err),
    }
}

async fn check_log_access(log_name: &str) -> Result<(), String> {
    let output = Command::new("powershell.exe")
        .arg("-NoProfile")
        .arg("-NonInteractive")
        .arg("-Command")
        .arg(format!(
            "Get-WinEvent -ListLog '{log_name}' -ErrorAction Stop | Out-Null"
        ))
        .output()
        .await
        .map_err(|_| "System event log unavailable".to_string())?;

    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}
