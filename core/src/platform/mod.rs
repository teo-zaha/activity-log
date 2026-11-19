use anyhow::{Result, anyhow};

pub fn unsupported_result<T>(message: &'static str) -> Result<T> {
    Err(anyhow!(message))
}

pub(crate) mod blocking;
#[cfg(target_os = "macos")]
pub mod macos;
pub mod private_file;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::{
    ComGuard, DispatchObject, active_outlook_application, connect_outlook_application,
    outlook_is_installed, process_is_elevated, query_windows_event_log, run_wsl_script,
    running_process_names, system_log_access, wsl_distribution_names,
};

/// A `Command` that never opens a console window, for subprocesses run by the GUI app.
pub fn background_command(program: &str) -> std::process::Command {
    #[allow(unused_mut)]
    let mut command = std::process::Command::new(program);
    #[cfg(target_os = "windows")]
    windows::hide_console_window(&mut command);
    command
}
