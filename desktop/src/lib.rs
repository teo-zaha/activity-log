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
    let runtime =
        tauri::async_runtime::block_on(CollectionRuntime::build_default()).expect("app runtime");
    let app_state = AppState {
        runtime: Arc::new(RwLock::new(runtime)),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::new().build())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::get_activity,
            commands::get_providers,
            commands::collect,
            commands::get_provider_config,
            commands::save_provider_config,
            commands::save_export
        ])
        .setup(move |app| {
            let app_handle = app.handle().clone();
            let source = load_store_env_config_source(&app_handle)
                .map_err(|err| -> Box<dyn std::error::Error> { err.into() })?;
            let runtime = tauri::async_runtime::block_on(
                CollectionRuntime::build_with_config_source(&source),
            )?;

            tauri::async_runtime::block_on(async {
                let state: tauri::State<'_, AppState> = app_handle.state();
                *state.runtime.write().await = runtime.clone();
            });

            if let Some(task) = runtime.initial_collection_task() {
                tauri::async_runtime::spawn(task);
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
