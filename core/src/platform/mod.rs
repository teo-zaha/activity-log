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
    ComGuard, local_date_bounds_as_systemtime, outlook_is_installed, query_windows_event_log,
    running_outlook_application, system_log_access,
};

/// A `Command` that never opens a console window, for subprocesses run by the GUI app.
pub fn background_command(program: &str) -> std::process::Command {
    #[allow(unused_mut)]
    let mut command = std::process::Command::new(program);
    #[cfg(target_os = "windows")]
    windows::hide_console_window(&mut command);
    command
}
