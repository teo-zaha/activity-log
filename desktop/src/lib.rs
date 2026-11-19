mod app_state;
mod commands;
mod deep_link;
mod devtools;
mod provider_config;
mod window_chrome;
mod window_state;

use std::time::{Duration, Instant};

use activity_log_core::settings::ConfigFile;
use tauri::{Manager, RunEvent, Runtime, WebviewWindow};

use app_state::AppState;

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
            if let Some(window) = app.get_webview_window(deep_link::MAIN_WINDOW) {
                window_state::restore_before_show(&window);
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
