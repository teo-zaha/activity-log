use anyhow::{Result, anyhow};

pub fn unsupported_result<T>(message: &'static str) -> Result<T> {
    Err(anyhow!(message))
}

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::{
    ComGuard, DispatchObject, create_outlook_application, local_date_bounds_as_systemtime,
    query_windows_event_log, system_log_access,
};
