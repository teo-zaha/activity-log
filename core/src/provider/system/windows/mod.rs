use std::process::Command;

pub enum SystemSupport {
    Available,
    Degraded(String),
    Unavailable(String),
}

pub fn check_system_support() -> SystemSupport {
    let system_ok = check_log_access("System");
    if let Err(err) = system_ok {
        return SystemSupport::Unavailable(err);
    }

    match check_log_access("Security") {
        Ok(()) => SystemSupport::Available,
        Err(_) => SystemSupport::Degraded("Security log access unavailable".to_string()),
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
