//! The main window opens at its last size and position, maximized or fullscreen as it was
//! (`tauri-plugin-window-state`, `.window-state.json` in the app's config folder). Not its
//! visibility: the window starts hidden and `index.html` shows it once its splash is styled. Not
//! its decorations: `tauri.windows.conf.json` decides them. A position on a screen that is gone
//! is dropped by the plugin, and the OS places the window.

#[cfg(test)]
mod tests;

use tauri::{Runtime, WebviewWindow, plugin::TauriPlugin};
use tauri_plugin_window_state::{Builder, StateFlags, WindowExt};

use crate::deep_link::MAIN_WINDOW;

/// What is saved on quit and restored at the next start.
pub(crate) const SAVED: StateFlags = StateFlags::SIZE
    .union(StateFlags::POSITION)
    .union(StateFlags::MAXIMIZED)
    .union(StateFlags::FULLSCREEN);

/// Restored while the window is still hidden, so it shows where it was.
pub(crate) const BEFORE_SHOW: StateFlags = SAVED.difference(StateFlags::FULLSCREEN);

/// Saves the window's state; the window restores it itself (`restore_before_show`), in two steps.
pub fn plugin<R: Runtime>() -> TauriPlugin<R> {
    Builder::new()
        .with_state_flags(SAVED)
        .skip_initial_state(MAIN_WINDOW)
        .build()
}

/// Size, position and maximized, before the page shows the window.
pub fn restore_before_show<R: Runtime>(window: &WebviewWindow<R>) {
    let _ = window.restore_state(BEFORE_SHOW);
}

/// Fullscreen waits until the window is on screen: on macOS entering it needs the window's screen
/// and can bring a hidden window up before its splash.
pub fn restore_fullscreen<R: Runtime>(window: &WebviewWindow<R>) {
    let _ = window.restore_state(StateFlags::FULLSCREEN);
}
