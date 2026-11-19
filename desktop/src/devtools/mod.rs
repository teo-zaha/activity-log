//! The web inspector while developing. The app hides the web view's right-click menu and with it
//! Inspect, so `ACTIVITY_LOG_DEVTOOLS=1` opens the inspector in a debug build; release builds have
//! none.

#[cfg(all(test, debug_assertions))]
mod tests;

use tauri::{Runtime, WebviewWindow};

pub fn open_if_asked<R: Runtime>(window: &WebviewWindow<R>) {
    open_impl(window);
}

#[cfg(debug_assertions)]
fn open_impl<R: Runtime>(window: &WebviewWindow<R>) {
    if asked(std::env::var_os("ACTIVITY_LOG_DEVTOOLS").as_deref()) {
        window.open_devtools();
    }
}

#[cfg(not(debug_assertions))]
fn open_impl<R: Runtime>(_window: &WebviewWindow<R>) {}

/// Whether `ACTIVITY_LOG_DEVTOOLS` asks for the inspector.
#[cfg(debug_assertions)]
pub(crate) fn asked(value: Option<&std::ffi::OsStr>) -> bool {
    value.is_some_and(|value| value == "1")
}
