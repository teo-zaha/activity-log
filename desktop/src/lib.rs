use activity_log_core::{
    model::{CollectRequest, CollectingResponse},
    runtime::{ActivitySnapshot, CollectionResult, CollectionRuntime, resolve_collect_options},
};
use rfd::FileDialog;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ExportFormat {
    Json,
    Markdown,
}

impl ExportFormat {
    fn extension(&self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Markdown => "md",
        }
    }

    fn filter_name(&self) -> &'static str {
        match self {
            Self::Json => "JSON",
            Self::Markdown => "Markdown",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportRequest {
    content: String,
    file_name: String,
    format: ExportFormat,
}

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

#[tauri::command]
fn save_export(request: ExportRequest) -> Result<(), String> {
    let selected_path = FileDialog::new()
        .set_title("Export activity log")
        .set_file_name(&request.file_name)
        .add_filter(request.format.filter_name(), &[request.format.extension()])
        .save_file();

    let Some(path) = selected_path else {
        return Ok(());
    };

    let path = ensure_extension(path, request.format.extension());
    std::fs::write(&path, request.content).map_err(|err| format!("Failed to save export: {err}"))
}

fn ensure_extension(mut path: PathBuf, extension: &str) -> PathBuf {
    let has_expected_extension = path
        .extension()
        .and_then(|current| current.to_str())
        .is_some_and(|current| current.eq_ignore_ascii_case(extension));

    if !has_expected_extension {
        path.set_extension(extension);
    }

    path
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
            collect,
            save_export
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
