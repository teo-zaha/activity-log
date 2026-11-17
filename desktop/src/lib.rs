mod app_state;
mod commands;
mod provider_config;

use activity_log_core::{diagnostics, runtime::CollectionRuntime};
use tauri::Manager;

use app_state::AppState;
use provider_config::{load_config, resolve_connections};

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::get_activity,
            commands::get_providers,
            commands::collect,
            commands::get_provider_config,
            commands::save_provider_config,
            commands::add_connection,
            commands::remove_connection,
            commands::save_export,
            commands::get_logs,
            commands::clear_logs
        ])
        .setup(move |app| {
            // A broken config file must not keep the app from opening, where it can be fixed.
            let connections = load_config()
                .and_then(|file| resolve_connections(&file))
                .unwrap_or_else(|err| {
                    diagnostics::error("settings", format!("Reading settings failed: {err}"));
                    Vec::new()
                });
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
