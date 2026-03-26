use std::process::Command;

pub enum SystemSupport {
    Available,
    Degraded(String),
    Unavailable(String),
}

pub fn check_system_support() -> SystemSupport {
    match check_log_access("System") {
        Ok(()) => SystemSupport::Available,
        Err(err) => SystemSupport::Unavailable(err),
    }
}

fn check_log_access(log_name: &str) -> Result<(), String> {
    let output = Command::new("powershell.exe")
        .arg("-NoProfile")
        .arg("-NonInteractive")
        .arg("-Command")
        .arg(format!(
            "Get-WinEvent -ListLog '{log_name}' -ErrorAction Stop | Out-Null"
        ))
        .output()
        .map_err(|_| "Windows event log unavailable".to_string())?;

    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}
