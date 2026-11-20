//! WebView2 keeps browser keys of its own: Ctrl+R and F5 reload the dashboard, Ctrl+P prints,
//! Alt+← goes back, Ctrl+F finds in the page, before or whatever the page does with them. The
//! desktop app turns them off on Windows, so the keys reach the dashboard, whose shortcuts
//! (`dashboard/src/app/models/shortcuts.ts`) collect, go back and filter. Copy, paste, select
//! all, undo and the other editing and movement keys are not browser keys and keep working.
//! WKWebView (macOS) and WebKitGTK (Linux) have no such keys.

#[cfg(target_os = "windows")]
mod windows;

#[cfg(test)]
mod tests;

use std::fmt::Display;

use tauri::{Runtime, WebviewWindow};

/// Leaves every key in `window`'s web view to the page. A failure leaves the browser keys on,
/// and the Logs pane says why.
pub fn leave_to_page<R: Runtime>(window: &WebviewWindow<R>) {
    leave_to_page_impl(window)
}

#[cfg(target_os = "windows")]
use self::windows::leave_to_page as leave_to_page_impl;

#[cfg(not(target_os = "windows"))]
fn leave_to_page_impl<R: Runtime>(_window: &WebviewWindow<R>) {}

/// The first failure of a key press, reported once with its cause: a runtime that takes the
/// handler but lacks what it uses fails the same way on every key.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
#[derive(Debug, Default)]
pub(crate) struct FirstFailure {
    reported: bool,
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
impl FirstFailure {
    /// What to log for `cause` the first time; nothing after.
    pub(crate) fn report(&mut self, cause: &dyn Display) -> Option<String> {
        if self.reported {
            return None;
        }
        self.reported = true;
        Some(format!(
            "A key in the web view keeps its browser action (reload, print, back): {cause}"
        ))
    }
}
