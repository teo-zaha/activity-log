use std::process::Command;

use crate::provider::system::windows::SystemSupport;

pub fn check_system_support() -> SystemSupport {
    match check_last_access() {
        Ok(()) => SystemSupport::Available,
        Err(err) => SystemSupport::Unavailable(format!("boot/shutdown history unavailable: {err}")),
    }
}

fn check_last_access() -> Result<(), String> {
    let output = Command::new("/usr/bin/last")
        .arg("-n")
        .arg("1")
        .output()
        .map_err(|err| err.to_string())?;

    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}
