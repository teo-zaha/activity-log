use activity_log_core::{
    diagnostics,
    export::render_export_request,
    model::{
        AddConnectionRequestDto, AddConnectionResponseDto, CollectRequest,
        CollectionFailedResponse, ConnectionId, ExportRequest, LogsResponse,
        ProviderConfigSaveRequestDto, ProviderConfigSaveResponseDto, ProviderConfigSnapshotDto,
        ProvidersResponse,
    },
    provider::current_os,
    runtime::{ActivitySnapshot, CollectionRuntime, resolve_collect_options},
    settings::ConfigFile,
};
use rfd::FileDialog;
use std::path::PathBuf;
use tauri::Url;
use tauri_plugin_opener::OpenerExt;

use crate::{
    app_state::AppState,
    provider_config::{
        apply_add, apply_save, build_snapshot, edited_connections, load_config, shown_connections,
    },
};

#[cfg(test)]
mod tests;

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

/// Runs a connection's startup check again and collects its kind again for the range on
/// screen; answers with the sources, this one checking.
#[tauri::command]
pub(crate) async fn recheck_connection(
    state: tauri::State<'_, AppState>,
    request: ConnectionId,
) -> Result<ProvidersResponse, String> {
    let runtime = state.runtime.read().await;
    let task = runtime
        .recheck(&request)
        .ok_or_else(|| format!("No source {request}"))?;
    // Before the task runs, so the answer shows the connection checking however fast it is.
    let response = runtime.providers_response();
    tauri::async_runtime::spawn(task);
    Ok(response)
}

#[tauri::command]
pub(crate) fn get_provider_config() -> Result<ProviderConfigSnapshotDto, String> {
    let file = load_config()?;
    Ok(build_snapshot(&shown_connections(&file)?, &current_os()))
}

#[tauri::command]
pub(crate) async fn save_provider_config(
    state: tauri::State<'_, AppState>,
    request: ProviderConfigSaveRequestDto,
) -> Result<ProviderConfigSaveResponseDto, String> {
    Ok(update_config(&state, |file| apply_save(file, &request))
        .await?
        .1)
}

#[tauri::command]
pub(crate) async fn add_connection(
    state: tauri::State<'_, AppState>,
    request: AddConnectionRequestDto,
) -> Result<AddConnectionResponseDto, String> {
    let (created, saved) = update_config(&state, |file| {
        let id = apply_add(file, &request)?;
        diagnostics::info("settings", format!("Added {id}"));
        Ok(id)
    })
    .await?;
    Ok(AddConnectionResponseDto { created, saved })
}

#[tauri::command]
pub(crate) async fn remove_connection(
    state: tauri::State<'_, AppState>,
    request: ConnectionId,
) -> Result<ProviderConfigSaveResponseDto, String> {
    let (_, saved) = update_config(&state, |file| {
        file.remove(&request).map_err(|err| format!("{err:#}"))?;
        diagnostics::info("settings", format!("Removed {request}"));
        Ok(())
    })
    .await?;
    Ok(saved)
}

/// Edits `config.toml`, saves it and re-runs the startup checks of every connection in the
/// background; the response lists the connections as checking. The new runtime takes over the
/// collection the old one was showing. Answers with what `change` returned and the new settings.
async fn update_config<T>(
    state: &AppState,
    change: impl FnOnce(&mut ConfigFile) -> Result<T, String>,
) -> Result<(T, ProviderConfigSaveResponseDto), String> {
    update_config_at(state, ConfigFile::resolve_path(None), change).await
}

async fn update_config_at<T>(
    state: &AppState,
    path: PathBuf,
    change: impl FnOnce(&mut ConfigFile) -> Result<T, String>,
) -> Result<(T, ProviderConfigSaveResponseDto), String> {
    // From reading the file to swapping the runtime: a second edit starts from this one's result.
    let _editing = state.config_edit.lock().await;
    let mut file = ConfigFile::load(path).map_err(|err| format!("{err:#}"))?;
    let changed = change(&mut file)?;
    file.save().map_err(|err| {
        let message = format!("Saving settings failed: {err:#}");
        diagnostics::error("settings", &message);
        message
    })?;
    diagnostics::info("settings", "Settings saved, re-running provider checks");

    let connections = edited_connections(&file)?;
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

    Ok((
        changed,
        ProviderConfigSaveResponseDto {
            snapshot: build_snapshot(&connections, &current_os()),
            providers_response,
        },
    ))
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

/// Renders the entries the dashboard shows and saves them where the user picks.
#[tauri::command]
pub(crate) fn save_export(request: ExportRequest) -> Result<(), String> {
    let file = render_export_request(&request).map_err(|err| format!("{err:#}"))?;
    let extension = request.format.extension();
    let selected_path = FileDialog::new()
        .set_title("Export activity log")
        .set_file_name(&file.file_name)
        .add_filter(request.format.label(), &[extension])
        .save_file();

    let Some(path) = selected_path else {
        return Ok(());
    };

    let path = ensure_extension(path, extension);
    std::fs::write(&path, file.content).map_err(|err| format!("Failed to save export: {err}"))
}

/// Opens a link from an event in the default browser. Descriptions come from whoever sent the
/// invitation, so only web links are opened, never files or other apps' schemes.
#[tauri::command]
pub(crate) fn open_url(app: tauri::AppHandle, request: String) -> Result<(), String> {
    let url = web_url(&request)?;
    app.opener()
        .open_url(url.as_str(), None::<&str>)
        .map_err(|err| {
            let message = format!("Opening a link failed: {err}");
            diagnostics::warn("desktop", &message);
            message
        })
}

fn web_url(raw: &str) -> Result<Url, String> {
    let url = Url::parse(raw.trim()).map_err(|_| "Not a valid link".to_string())?;
    match url.scheme() {
        "https" | "http" if url.host_str().is_some_and(|host| !host.is_empty()) => Ok(url),
        "https" | "http" => Err("The link has no host".to_string()),
        other => Err(format!("Only web links can be opened, not {other}: links")),
    }
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
