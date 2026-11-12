use activity_log_core::{
    diagnostics,
    model::{
        CollectRequest, CollectingResponse, LogsResponse, ProviderConfigSaveRequestDto,
        ProviderConfigSaveResponseDto, ProviderConfigSnapshotDto, ProvidersResponse,
    },
    runtime::{ActivitySnapshot, CollectionResult, CollectionRuntime, resolve_collect_options},
};
use rfd::FileDialog;
use serde::Deserialize;
use std::path::PathBuf;

use crate::{
    app_state::AppState,
    provider_config::{
        StoreEnvConfigSource, apply_provider_config_updates, build_provider_config_snapshot,
        load_store_env_config_source, read_store_provider_config, write_store_provider_config,
    },
};

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
pub(crate) struct ExportRequest {
    content: String,
    file_name: String,
    format: ExportFormat,
}

#[tauri::command]
pub(crate) async fn get_activity(
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let runtime = state.runtime.read().await.clone();

    match runtime.activity_snapshot().await {
        ActivitySnapshot::Collecting => {
            Ok(serde_json::to_value(CollectingResponse { collecting: true }).unwrap())
        }
        ActivitySnapshot::Ready(log) => Ok(serde_json::to_value(log).unwrap()),
        ActivitySnapshot::Empty => Err("No data yet".to_string()),
    }
}

#[tauri::command]
pub(crate) async fn get_providers(
    state: tauri::State<'_, AppState>,
) -> Result<ProvidersResponse, String> {
    Ok(state.runtime.read().await.providers_response())
}

#[tauri::command]
pub(crate) async fn collect(
    state: tauri::State<'_, AppState>,
    request: CollectRequest,
) -> Result<serde_json::Value, String> {
    let runtime = state.runtime.read().await.clone();
    let options = resolve_collect_options(&request).map_err(str::to_string)?;

    match runtime.collect_with_options(options).await {
        Ok(CollectionResult::Ready(log)) => Ok(serde_json::to_value(log).unwrap()),
        Ok(CollectionResult::Collecting) => {
            Ok(serde_json::to_value(CollectingResponse { collecting: true }).unwrap())
        }
        Err(err) => Err(format!("{err:#}")),
    }
}

#[tauri::command]
pub(crate) fn get_provider_config(
    app: tauri::AppHandle,
) -> Result<ProviderConfigSnapshotDto, String> {
    let config_source = load_store_env_config_source(&app)?;
    Ok(build_provider_config_snapshot(&config_source))
}

#[tauri::command]
pub(crate) async fn save_provider_config(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    request: ProviderConfigSaveRequestDto,
) -> Result<ProviderConfigSaveResponseDto, String> {
    let mut store_values = read_store_provider_config(&app)?;
    apply_provider_config_updates(&mut store_values, &request)?;
    write_store_provider_config(&app, &store_values).inspect_err(|err| {
        diagnostics::error("settings", format!("Saving settings failed: {err}"));
    })?;
    diagnostics::info("settings", "Settings saved, re-running provider checks");

    let config_source = StoreEnvConfigSource {
        store_values: store_values.clone(),
    };
    let runtime = CollectionRuntime::build_with_config_source(&config_source)
        .await
        .map_err(|err| {
            let message = format!("Failed to refresh runtime config: {err}");
            diagnostics::error("settings", &message);
            message
        })?;
    let providers_response = runtime.providers_response();

    {
        let mut runtime_slot = state.runtime.write().await;
        *runtime_slot = runtime;
    }

    let snapshot = build_provider_config_snapshot(&config_source);
    Ok(ProviderConfigSaveResponseDto {
        snapshot,
        providers_response,
    })
}

#[tauri::command]
pub(crate) fn get_logs() -> LogsResponse {
    LogsResponse {
        entries: diagnostics::entries(),
    }
}

#[tauri::command]
pub(crate) fn clear_logs() {
    diagnostics::clear();
}

#[tauri::command]
pub(crate) fn save_export(request: ExportRequest) -> Result<(), String> {
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
