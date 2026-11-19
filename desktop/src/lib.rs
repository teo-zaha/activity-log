mod app_state;
mod commands;
mod deep_link;
mod provider_config;
mod window_chrome;

use std::time::Duration;

use activity_log_core::runtime::CollectionRuntime;
use tauri::{AppHandle, Manager, Runtime};

use app_state::AppState;
use provider_config::startup_connections;

pub fn run() {
    tauri::Builder::default()
        // First, so a second launch hands its link (Windows, Linux) to this app and quits.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            deep_link::show_main_window(app);
        }))
        .plugin(tauri_plugin_deep_link::init())
        // Only `open_url` opens links, with its own check; the plugin's commands stay without a
        // permission and its click interception is off.
        .plugin(
            tauri_plugin_opener::Builder::new()
                .open_js_links_on_click(false)
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            commands::get_activity,
            commands::get_providers,
            commands::collect,
            commands::get_provider_config,
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
        ])
        .setup(move |app| {
            deep_link::setup(app.handle());
            show_window_fallback(app.handle().clone());
            let connections = startup_connections(None);
            // The startup checks can take seconds (WSL starting, a large disk, a slow host), so
            // they run in the background: until its own check finishes a source is listed as
            // checking, and the first collection reads each source as soon as it is checked.
            let (runtime, checks) = CollectionRuntime::start(&connections);
            let initial = runtime.initial_collection_task();
            app.manage(AppState::new(runtime));
            tauri::async_runtime::spawn(checks);
            if let Some(task) = initial {
                tauri::async_runtime::spawn(task);
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// How long the hidden window waits for the dashboard to show it.
const SHOW_WINDOW_FALLBACK: Duration = Duration::from_secs(2);

/// The window starts hidden and `index.html` shows it as soon as its splash is styled, so there
/// is no blank frame first. If the page never gets that far, show the window anyway.
fn show_window_fallback<R: Runtime>(app: AppHandle<R>) {
    std::thread::spawn(move || {
        std::thread::sleep(SHOW_WINDOW_FALLBACK);
        if let Some(window) = app.get_webview_window(deep_link::MAIN_WINDOW)
            && !window.is_visible().unwrap_or(false)
        {
            let _ = window.show();
        }
    });
}
