//! The Windows window has no title bar and the dashboard draws its buttons
//! (`components/window-controls/`). Windows 11 shows its snap layouts only over a window that
//! says it is a maximize button, and the page lives in WebView2's own window, so `windows` puts
//! an invisible window of that kind over the dashboard's maximize button.

#[cfg(target_os = "windows")]
mod windows;

#[cfg(test)]
mod tests;

use tauri::{Runtime, WebviewWindow};

/// The dashboard's maximize button in CSS pixels, from the top and from the right edge of the
/// window: it keeps that place as the window resizes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ButtonArea {
    pub top: f64,
    pub right: f64,
    pub width: f64,
    pub height: f64,
}

/// `area` as `(x, y, width, height)` in physical pixels of a client area `client_width` wide.
/// The top `frame` pixels stay with the border that resizes the window.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) fn placement(
    area: ButtonArea,
    client_width: i32,
    scale: f64,
    frame: i32,
) -> (i32, i32, i32, i32) {
    let px = |value: f64| (value * scale).round() as i32;
    let right = client_width - px(area.right);
    let left = right - px(area.width);
    let top = px(area.top).max(frame);
    let bottom = px(area.top + area.height);
    (left, top, right - left, (bottom - top).max(0))
}

/// Gives the dashboard's maximize button the snap layouts of Windows 11; nothing elsewhere. The
/// button's hover and press come back as the `maximize-button` event, and a click maximizes or
/// restores the window. A failure leaves the page's own button, and the Logs pane says why.
#[tauri::command]
pub fn attach_maximize_button<R: Runtime>(
    window: WebviewWindow<R>,
    top: f64,
    right: f64,
    width: f64,
    height: f64,
) {
    attach_impl(
        window,
        ButtonArea {
            top,
            right,
            width,
            height,
        },
    )
}

#[cfg(target_os = "windows")]
use self::windows::attach as attach_impl;

#[cfg(not(target_os = "windows"))]
fn attach_impl<R: Runtime>(_window: WebviewWindow<R>, _area: ButtonArea) {}
