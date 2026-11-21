//! `activity-log://` links from the OS. The dashboard turns a link into a navigation
//! (`DeepLinkService.open`); this module only gets it there. A link that launched the app is held
//! until the dashboard asks for it with `take_launch_deep_link`, later ones are sent as the
//! `deep-link` event and bring the window to the front.

use std::sync::Mutex;

use activity_log_core::{diagnostics, ipc::DEEP_LINK_EVENT};
use tauri::{AppHandle, Emitter, Manager, Runtime, Url};
use tauri_plugin_deep_link::DeepLinkExt;

#[cfg(test)]
mod tests;

/// The scheme `tauri.conf.json` registers.
pub(crate) const SCHEME: &str = "activity-log";
pub(crate) const MAIN_WINDOW: &str = "main";

/// The first link of ours among `urls`; anything else the OS hands over is dropped.
pub(crate) fn app_link(urls: impl IntoIterator<Item = Url>) -> Option<String> {
    urls.into_iter()
        .find(|url| url.scheme() == SCHEME)
        .map(String::from)
}

/// Links that arrive before the dashboard is ready for them.
#[derive(Default)]
pub(crate) struct DeepLinkInbox(Mutex<InboxState>);

#[derive(Default)]
struct InboxState {
    /// The dashboard has asked for the launch link, so it listens for the event.
    ready: bool,
    pending: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Delivery {
    /// Kept for `take`; a later link replaces it.
    Held,
    /// To be sent as the event now.
    Forward(String),
}

impl Delivery {
    /// Whether a hidden window may be shown for this link. A held link waits for a page that is
    /// still starting; the window starts hidden and the page shows it once its splash is styled
    /// (or `show_window_fallback` does), so showing it now would show a blank frame first.
    pub(crate) fn shows_window(&self) -> bool {
        matches!(self, Delivery::Forward(_))
    }
}

impl DeepLinkInbox {
    pub(crate) fn receive(&self, link: String) -> Delivery {
        let mut state = self.lock();
        if state.ready {
            Delivery::Forward(link)
        } else {
            state.pending = Some(link);
            Delivery::Held
        }
    }

    /// The link held so far, once; from now on links are forwarded.
    pub(crate) fn take(&self) -> Option<String> {
        let mut state = self.lock();
        state.ready = true;
        state.pending.take()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, InboxState> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Wires the plugin up: the launch link, links while running and, in debug builds on Windows and
/// Linux, the scheme registration an installer would do.
pub(crate) fn setup<R: Runtime>(app: &AppHandle<R>) {
    app.manage(DeepLinkInbox::default());
    let deep_link = app.deep_link();
    #[cfg(all(debug_assertions, any(target_os = "windows", target_os = "linux")))]
    if let Err(err) = deep_link.register_all() {
        diagnostics::warn(
            "desktop",
            format!("Registering {SCHEME}: links failed: {err}"),
        );
    }
    if let Ok(Some(urls)) = deep_link.get_current() {
        receive(app, urls);
    }
    let handle = app.clone();
    deep_link.on_open_url(move |event| receive(&handle, event.urls()));
}

fn receive<R: Runtime>(app: &AppHandle<R>, urls: Vec<Url>) {
    let Some(link) = app_link(urls) else {
        diagnostics::warn(
            "desktop",
            "Ignored a link that is not an activity-log: link",
        );
        return;
    };
    let delivery = app.state::<DeepLinkInbox>().receive(link);
    let shows_window = delivery.shows_window();
    if let Delivery::Forward(link) = delivery
        && let Err(err) = app.emit_to(MAIN_WINDOW, DEEP_LINK_EVENT, link)
    {
        diagnostics::warn(
            "desktop",
            format!("Passing a link to the window failed: {err}"),
        );
    }
    if shows_window {
        show_main_window(app);
    } else {
        focus_main_window_if_visible(app);
    }
}

/// Brings the window to the front, for a link or a second launch.
pub(crate) fn show_main_window<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window(MAIN_WINDOW) else {
        return;
    };
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
}

/// Brings the window to the front only if it is already on screen.
fn focus_main_window_if_visible<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW)
        && window.is_visible().unwrap_or(false)
    {
        let _ = window.set_focus();
    }
}

/// The link that launched the app, if any; the dashboard asks once it listens for the event.
#[tauri::command]
pub(crate) fn take_launch_deep_link(inbox: tauri::State<'_, DeepLinkInbox>) -> Option<String> {
    inbox.take()
}
