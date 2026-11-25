mod app_menu;
mod app_state;
mod browser_keys;
mod collection_notice;
mod commands;
mod deep_link;
mod devtools;
mod fix_action;
mod legacy_import;
mod provider_config;
mod runtime_events;
mod window_chrome;
mod window_state;

use std::time::{Duration, Instant};

use activity_log_core::settings::ConfigFile;
use tauri::{
    AppHandle, Emitter, Manager, RunEvent, Runtime, WebviewWindow, webview::PageLoadEvent,
};
use tauri_plugin_notification::NotificationExt;

use app_state::AppState;

/// Every command the dashboard calls. Tauri names each by its function; `core::ipc` and
/// `core::menu` name them for the dashboard, and `tests.rs` checks both name the same.
macro_rules! app_commands {
    () => {
        tauri::generate_handler![
            app_menu::install_menu,
            app_menu::update_menu,
            commands::get_runtime_state,
            commands::collect,
            commands::get_provider_config,
            commands::check_provider_config,
            commands::save_provider_config,
            commands::add_connection,
            commands::remove_connection,
            commands::recheck_connection,
            commands::save_export,
            commands::open_url,
            fix_action::open_local_fix,
            commands::get_logs,
            commands::clear_logs,
            deep_link::take_launch_deep_link,
            legacy_import::take_legacy_import_report,
            legacy_import::import_legacy_settings,
            window_chrome::attach_maximize_button
        ]
    };
}

#[cfg(test)]
mod tests;

pub fn run() {
    // First, so every path below is the profile's. Only a debug build refuses a value, before
    // there is a window to say it in.
    let profile = match activity_log_core::profile::init(None) {
        Ok(profile) => profile,
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(2);
        }
    };
    tauri::Builder::default()
        // First, so a second launch hands its link (Windows, Linux) to this app and quits.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            deep_link::show_main_window(app);
        }))
        .plugin(tauri_plugin_deep_link::init())
        .plugin(window_state::plugin())
        // Shown from Rust only (`collection_notice`): the page may not notify. The plugin still
        // injects its `window.Notification` into every page, which asks whether notifications
        // are permitted as it loads; the capability allows that question alone (on the desktop
        // the answer is always yes, no prompt), so no load leaves a refused call behind.
        .plugin(tauri_plugin_notification::init())
        // Only `open_url` opens links, with its own check; the plugin's commands stay without a
        // permission and its click interception is off.
        .plugin(
            tauri_plugin_opener::Builder::new()
                .open_js_links_on_click(false)
                .build(),
        )
        .manage(app_menu::AppMenu::<tauri::Wry>::default())
        .manage(commands::ExportTurn::default())
        .manage(legacy_import::ImportReport::default())
        .on_menu_event(|app, event| app_menu::chosen(app, event.id()))
        .on_page_load(|webview, payload| {
            if payload.event() == PageLoadEvent::Started {
                page_started(webview.label(), webview.app_handle());
            }
        })
        .invoke_handler(app_commands!())
        .setup(move |app| {
            deep_link::setup(app.handle());
            if let Some(window) = app.get_webview_window(deep_link::MAIN_WINDOW) {
                if profile == activity_log_core::profile::Profile::Dev {
                    show_dev_title(&window);
                }
                window_state::restore_before_show(&window);
                browser_keys::leave_to_page(&window);
                devtools::open_if_asked(&window);
                after_show(window);
            }
            // The startup checks can take seconds (WSL starting, a large disk, a slow host), so
            // they run in the background: until its own check finishes a source is listed as
            // checking, and the first collection reads each source as soon as it is checked.
            let path = ConfigFile::resolve_path(None);
            // The one place the app builds its stores of the key.
            let secrets = activity_log_core::secrets::Secrets::desktop(path.as_deref());
            // Before the host first reads the file, while nothing else asks the keychain: 0.0.7's
            // settings once, then plain text sealed.
            let report = legacy_import::import_before_host(&secrets);
            app.state::<legacy_import::ImportReport>().hold(report);
            if let Some(path) = &path {
                tauri::async_runtime::block_on(
                    activity_log_core::settings::startup_seal::seal_without_asking(path, &secrets),
                );
            }
            let (state, run_ends) = AppState::start_following(path, secrets);
            state.host.start_initial_collection();
            // Edits from the CLI apply within a few seconds.
            tauri::async_runtime::spawn(state.host.watch_config());
            // The host's state as events to the window, until the host shuts down.
            let window = app.handle().clone();
            let feed = futures::stream::unfold(state.host.feed(), |mut feed| async move {
                feed.next().await.map(|value| (value, feed))
            });
            tauri::async_runtime::spawn(runtime_events::forward(
                Box::pin(feed),
                move |event, json| window.emit_str_to(deep_link::MAIN_WINDOW, event, json),
            ));
            // A long collection that ends while the window is in the background says so.
            let app_handle = app.handle().clone();
            let host = state.host.clone();
            tauri::async_runtime::spawn(collection_notice::follow(
                run_ends,
                {
                    let app = app_handle.clone();
                    move || main_window_focused(&app)
                },
                move |run| host.is_current(run),
                move |notice| {
                    if let Err(err) = app_handle
                        .notification()
                        .builder()
                        .title(notice.title)
                        .body(notice.body)
                        .show()
                    {
                        activity_log_core::diagnostics::warn(
                            "desktop",
                            format!("Showing a notification failed: {err}"),
                        );
                    }
                },
            ));
            app.manage(state);

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let RunEvent::Exit = event
                && let Some(state) = app.try_state::<AppState>()
            {
                state.host.shutdown();
            }
        });
}

/// A page that loads again (a reload) in the main window starts with the default menu and
/// installs its own, and asks for the links that came meanwhile.
fn page_started<R: Runtime>(label: &str, app: &AppHandle<R>) {
    if label == deep_link::MAIN_WINDOW {
        app_menu::page_loading(app);
        deep_link::page_loading(app);
    }
}

/// Whether the main window has focus now, for `collection_notice`; `WindowNotFound` once it is
/// gone.
fn main_window_focused<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<bool> {
    app.get_webview_window(deep_link::MAIN_WINDOW)
        .ok_or(tauri::Error::WindowNotFound)?
        .is_focused()
}

/// How long the hidden window waits for the dashboard to show it.
const SHOW_WINDOW_FALLBACK: Duration = Duration::from_secs(2);
const SHOW_POLL: Duration = Duration::from_millis(50);

/// The window starts hidden and `index.html` shows it as soon as its splash is styled, so there
/// is no blank frame first. If the page never gets that far, show the window anyway. Once it is
/// on screen it goes fullscreen if it was.
fn after_show<R: Runtime>(window: WebviewWindow<R>) {
    std::thread::spawn(move || {
        let deadline = Instant::now() + SHOW_WINDOW_FALLBACK;
        while !window.is_visible().unwrap_or(false) {
            if Instant::now() >= deadline {
                warn_if_failed("Showing the window", window.show());
                break;
            }
            std::thread::sleep(SHOW_POLL);
        }
        window_state::restore_fullscreen(&window);
    });
}

/// Logs what a window or plugin call that failed was for: the app goes on without it, and the
/// Logs pane says so.
pub(crate) fn warn_if_failed<T, E: std::fmt::Display>(action: &str, result: Result<T, E>) {
    if let Err(err) = result {
        activity_log_core::diagnostics::warn("desktop", format!("{action} failed: {err}"));
    }
}

/// The dev profile's window says so in its title.
fn show_dev_title(window: &tauri::WebviewWindow) {
    let title = window
        .title()
        .map(|title| format!("{title} (dev)"))
        .unwrap_or_else(|_| "Activity Log (dev)".to_string());
    if let Err(err) = window.set_title(&title) {
        activity_log_core::diagnostics::warn("desktop", format!("Setting the title failed: {err}"));
    }
}
