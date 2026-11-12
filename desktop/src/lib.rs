mod app_state;
mod commands;
mod provider_config;

use activity_log_core::runtime::CollectionRuntime;
use std::sync::Arc;
use tauri::Manager;
use tokio::sync::RwLock;

use app_state::AppState;
use provider_config::load_store_env_config_source;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            commands::get_activity,
            commands::get_providers,
            commands::collect,
            commands::get_provider_config,
            commands::save_provider_config,
            commands::save_export,
            commands::get_logs,
            commands::clear_logs
        ])
        .setup(move |app| {
            // Built once from the settings store (with env fallback): startup checks
            // can launch Outlook or walk the disk, so they must not run twice.
            let source = load_store_env_config_source(app.handle())
                .map_err(|err| -> Box<dyn std::error::Error> { err.into() })?;
            let runtime = tauri::async_runtime::block_on(
                CollectionRuntime::build_with_config_source(&source),
            )?;
            app.manage(AppState {
                runtime: Arc::new(RwLock::new(runtime.clone())),
            });

            if let Some(task) = runtime.initial_collection_task() {
                tauri::async_runtime::spawn(task);
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
