mod app_menu;
mod app_state;
mod browser_keys;
mod commands;
mod deep_link;
mod devtools;
mod provider_config;
mod runtime_events;
mod window_chrome;
mod window_state;

use std::time::{Duration, Instant};

use activity_log_core::settings::ConfigFile;
use tauri::{Emitter, Manager, RunEvent, Runtime, WebviewWindow, webview::PageLoadEvent};

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
            commands::get_logs,
            commands::clear_logs,
            deep_link::take_launch_deep_link,
            window_chrome::attach_maximize_button
        ]
    };
}

#[cfg(test)]
mod tests;

pub fn run() {
    tauri::Builder::default()
        // First, so a second launch hands its link (Windows, Linux) to this app and quits.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            deep_link::show_main_window(app);
        }))
        .plugin(tauri_plugin_deep_link::init())
        .plugin(window_state::plugin())
        // Only `open_url` opens links, with its own check; the plugin's commands stay without a
        // permission and its click interception is off.
        .plugin(
            tauri_plugin_opener::Builder::new()
                .open_js_links_on_click(false)
                .build(),
        )
        .manage(app_menu::AppMenu::<tauri::Wry>::default())
        .on_menu_event(|app, event| app_menu::chosen(app, event.id()))
        // A page that loads again starts with the default menu and installs its own.
        .on_page_load(|webview, payload| {
            if webview.label() == deep_link::MAIN_WINDOW
                && payload.event() == PageLoadEvent::Started
            {
                app_menu::page_loading(webview.app_handle());
            }
        })
        .invoke_handler(app_commands!())
        .setup(move |app| {
            deep_link::setup(app.handle());
            if let Some(window) = app.get_webview_window(deep_link::MAIN_WINDOW) {
                window_state::restore_before_show(&window);
                browser_keys::leave_to_page(&window);
                devtools::open_if_asked(&window);
                after_show(window);
            }
            // The startup checks can take seconds (WSL starting, a large disk, a slow host), so
            // they run in the background: until its own check finishes a source is listed as
            // checking, and the first collection reads each source as soon as it is checked.
            let state = AppState::start(ConfigFile::resolve_path(None));
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
                let _ = window.show();
                break;
            }
            std::thread::sleep(SHOW_POLL);
        }
        window_state::restore_fullscreen(&window);
    });
}
