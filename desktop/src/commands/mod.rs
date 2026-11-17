use activity_log_core::{
    diagnostics,
    model::{
        AddConnectionRequestDto, CollectRequest, CollectionFailedResponse, ConnectionId,
        LogsResponse, ProviderConfigSaveRequestDto, ProviderConfigSaveResponseDto,
        ProviderConfigSnapshotDto, ProvidersResponse,
    },
    provider::current_os,
    runtime::{ActivitySnapshot, CollectionRuntime, resolve_collect_options},
    settings::ConfigFile,
};
use rfd::FileDialog;
use serde::Deserialize;
use std::path::PathBuf;

use crate::{
    app_state::AppState,
    provider_config::{apply_add, apply_save, build_snapshot, load_config, resolve_connections},
};

#[cfg(test)]
mod tests;

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

/// A running collection answers with the log of what is in so far, like a finished one.
fn activity_response(snapshot: ActivitySnapshot) -> Result<serde_json::Value, String> {
    match snapshot {
        ActivitySnapshot::Running(log) | ActivitySnapshot::Ready(log) => {
            Ok(serde_json::to_value(&*log).unwrap())
        }
        ActivitySnapshot::Failed { options, message } => {
            Ok(serde_json::to_value(CollectionFailedResponse::new(&options, message)).unwrap())
        }
        ActivitySnapshot::Empty => Err("No data yet".to_string()),
    }
}

#[tauri::command]
pub(crate) async fn get_activity(
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    activity_response(state.runtime.read().await.activity_snapshot())
}

#[tauri::command]
pub(crate) async fn get_providers(
    state: tauri::State<'_, AppState>,
) -> Result<ProvidersResponse, String> {
    Ok(state.runtime.read().await.providers_response())
}

/// Starts the collection and answers at once; `get_activity` shows it filling up.
#[tauri::command]
pub(crate) async fn collect(
    state: tauri::State<'_, AppState>,
    request: CollectRequest,
) -> Result<serde_json::Value, String> {
    let options = resolve_collect_options(&request).map_err(str::to_string)?;
    // Started under the lock, so a settings save cannot swap the runtime in between.
    let runtime = state.runtime.read().await;
    let (snapshot, task) = runtime.start_collection(options);
    if let Some(task) = task {
        tauri::async_runtime::spawn(task);
    }
    activity_response(snapshot)
}

#[tauri::command]
pub(crate) fn get_provider_config() -> Result<ProviderConfigSnapshotDto, String> {
    let file = load_config()?;
    Ok(build_snapshot(&resolve_connections(&file)?, &current_os()))
}

#[tauri::command]
pub(crate) async fn save_provider_config(
    state: tauri::State<'_, AppState>,
    request: ProviderConfigSaveRequestDto,
) -> Result<ProviderConfigSaveResponseDto, String> {
    update_config(&state, |file| apply_save(file, &request)).await
}

#[tauri::command]
pub(crate) async fn add_connection(
    state: tauri::State<'_, AppState>,
    request: AddConnectionRequestDto,
) -> Result<ProviderConfigSaveResponseDto, String> {
    update_config(&state, |file| {
        let id = apply_add(file, &request)?;
        diagnostics::info("settings", format!("Added {id}"));
        Ok(())
    })
    .await
}

#[tauri::command]
pub(crate) async fn remove_connection(
    state: tauri::State<'_, AppState>,
    request: ConnectionId,
) -> Result<ProviderConfigSaveResponseDto, String> {
    update_config(&state, |file| {
        file.remove(&request).map_err(|err| format!("{err:#}"))?;
        diagnostics::info("settings", format!("Removed {request}"));
        Ok(())
    })
    .await
}

/// Edits `config.toml`, saves it and re-runs the startup checks of every connection in the
/// background; the response lists the connections as checking. The new runtime takes over the
/// collection the old one was showing.
async fn update_config(
    state: &AppState,
    change: impl FnOnce(&mut ConfigFile) -> Result<(), String>,
) -> Result<ProviderConfigSaveResponseDto, String> {
    update_config_at(state, ConfigFile::resolve_path(None), change).await
}

async fn update_config_at(
    state: &AppState,
    path: PathBuf,
    change: impl FnOnce(&mut ConfigFile) -> Result<(), String>,
) -> Result<ProviderConfigSaveResponseDto, String> {
    // From reading the file to swapping the runtime: a second edit starts from this one's result.
    let _editing = state.config_edit.lock().await;
    let mut file = ConfigFile::load(path).map_err(|err| format!("{err:#}"))?;
    change(&mut file)?;
    file.save().map_err(|err| {
        let message = format!("Saving settings failed: {err:#}");
        diagnostics::error("settings", &message);
        message
    })?;
    diagnostics::info("settings", "Settings saved, re-running provider checks");

    let connections = resolve_connections(&file)?;
    let (runtime, checks) = CollectionRuntime::start(&connections);
    tauri::async_runtime::spawn(checks);
    let providers_response = runtime.providers_response();
    {
        let mut current = state.runtime.write().await;
        if let Some(task) = runtime.take_over(&current) {
            tauri::async_runtime::spawn(task);
        }
        *current = runtime;
    }

    Ok(ProviderConfigSaveResponseDto {
        snapshot: build_snapshot(&connections, &current_os()),
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
