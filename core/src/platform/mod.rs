use anyhow::{Result, anyhow};

pub fn unsupported_result<T>(message: &'static str) -> Result<T> {
    Err(anyhow!(message))
}

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::powershell_command;
