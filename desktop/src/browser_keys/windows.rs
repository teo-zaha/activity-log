use ::windows::core::{Interface, Result};
use activity_log_core::diagnostics;
use tauri::{Runtime, WebviewWindow};
use webview2_com::{
    AcceleratorKeyPressedEventHandler,
    Microsoft::Web::WebView2::Win32::{
        ICoreWebView2AcceleratorKeyPressedEventArgs, ICoreWebView2AcceleratorKeyPressedEventArgs2,
        ICoreWebView2Controller,
    },
};

use super::FirstFailure;

/// Turns the browser's action off for each key as it is pressed, rather than through
/// `ICoreWebView2Settings3::SetAreBrowserAcceleratorKeysEnabled`: a setting changed once the web
/// view exists applies only from its next navigation, after the dashboard has loaded. The key
/// still reaches the page (the handler does not mark it handled).
pub fn leave_to_page<R: Runtime>(window: &WebviewWindow<R>) {
    let queued = window.with_webview(|webview| {
        if let Err(err) = unsafe { listen(&webview.controller()) } {
            unavailable(&err.to_string());
        }
    });
    if let Err(err) = queued {
        unavailable(&err.to_string());
    }
}

unsafe fn listen(controller: &ICoreWebView2Controller) -> Result<()> {
    let mut failure = FirstFailure::default();
    let handler = AcceleratorKeyPressedEventHandler::create(Box::new(move |_, args| {
        let Some(args) = args else { return Ok(()) };
        if let Err(err) = unsafe { leave_key(&args) }
            && let Some(message) = failure.report(&err)
        {
            diagnostics::warn("window", message);
        }
        Ok(())
    }));
    let mut token = 0;
    unsafe { controller.add_AcceleratorKeyPressed(&handler, &mut token) }
}

/// Turns the browser's action off for this key; the interface needs WebView2 1.0.1293 or later.
unsafe fn leave_key(args: &ICoreWebView2AcceleratorKeyPressedEventArgs) -> Result<()> {
    unsafe {
        args.cast::<ICoreWebView2AcceleratorKeyPressedEventArgs2>()?
            .SetIsBrowserAcceleratorKeyEnabled(false)
    }
}

fn unavailable(reason: &str) {
    diagnostics::warn(
        "window",
        format!("The web view's browser keys (reload, print, back) stay on: {reason}"),
    );
}
