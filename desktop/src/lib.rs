use activity_log_core::{
    model::{CollectRequest, CollectingResponse},
    runtime::{ActivitySnapshot, CollectionResult, CollectionRuntime, resolve_collect_options},
};

#[tauri::command]
async fn get_activity(
    runtime: tauri::State<'_, CollectionRuntime>,
) -> Result<serde_json::Value, String> {
    match runtime.activity_snapshot().await {
        ActivitySnapshot::Collecting => {
            Ok(serde_json::to_value(CollectingResponse { collecting: true }).unwrap())
        }
        ActivitySnapshot::Ready(log) => Ok(serde_json::to_value(log).unwrap()),
        ActivitySnapshot::Empty => Err("No data yet".to_string()),
    }
}

#[tauri::command]
fn get_providers(
    runtime: tauri::State<'_, CollectionRuntime>,
) -> activity_log_core::model::ProvidersResponse {
    runtime.providers_response()
}

#[tauri::command]
async fn collect(
    runtime: tauri::State<'_, CollectionRuntime>,
    request: CollectRequest,
) -> Result<serde_json::Value, String> {
    let options = resolve_collect_options(&request).map_err(str::to_string)?;

    match runtime.collect_with_options(options).await {
        Ok(CollectionResult::Ready(log)) => Ok(serde_json::to_value(log).unwrap()),
        Ok(CollectionResult::Collecting) => {
            Ok(serde_json::to_value(CollectingResponse { collecting: true }).unwrap())
        }
        Err(err) => Err(format!("Collection failed: {err}")),
    }
}

pub fn run() {
    let runtime =
        tauri::async_runtime::block_on(CollectionRuntime::build_default()).expect("app runtime");
    let initial_runtime = runtime.clone();

    tauri::Builder::default()
        .manage(runtime)
        .invoke_handler(tauri::generate_handler![
            get_activity,
            get_providers,
            collect
        ])
        .setup(move |_app| {
            if let Some(task) = initial_runtime.initial_collection_task() {
                tauri::async_runtime::spawn(task);
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
