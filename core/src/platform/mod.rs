pub(crate) mod blocking;
pub(crate) mod dirs;
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

#[cfg(test)]
mod tests;
#[cfg(test)]
pub(crate) mod thread_time;

/// A `background_command` whose output is parsed: it runs in the C locale, so the dates and
/// words it prints are English whatever language the user runs (AGENTS.md).
pub fn parsed_output_command(program: &str) -> std::process::Command {
    let mut command = background_command(program);
    command.env("LC_ALL", "C");
    command
}

/// A `Command` that never opens a console window, for subprocesses run by the GUI app.
pub fn background_command(program: &str) -> std::process::Command {
    #[allow(unused_mut)]
    let mut command = std::process::Command::new(program);
    #[cfg(target_os = "windows")]
    windows::hide_console_window(&mut command);
    command
}
