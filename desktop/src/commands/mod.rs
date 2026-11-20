use activity_log_core::{
    diagnostics,
    export::render_export_request,
    model::{
        ActivityState, AddConnectionRequestDto, AddConnectionResponseDto, CollectAnswer,
        CollectRequest, ConnectionId, ExportRequest, LogsResponse, ProviderConfigSaveRequestDto,
        ProviderConfigSaveResponseDto, ProviderConfigSnapshotDto, ProvidersResponse, RecheckAnswer,
        RuntimeState, ShownActivity,
    },
    provider::current_os,
    runtime::resolve_collect_options,
    settings::{ConfigFile, EditError, Precondition},
};
use rfd::FileDialog;
use tauri::Url;
use tauri_plugin_opener::OpenerExt;

use crate::{
    app_state::AppState,
    provider_config::{apply_add, apply_save, build_snapshot, load_config, shown_connections},
};

#[cfg(test)]
mod tests;

/// A running collection answers with the log of what is in so far, like a finished one.
fn activity_response(state: ActivityState) -> Result<serde_json::Value, String> {
    match state {
        ActivityState::Running { log } | ActivityState::Ready { log } => {
            Ok(serde_json::to_value(&*log).unwrap())
        }
        ActivityState::Failed { failure } => Ok(serde_json::to_value(failure).unwrap()),
        ActivityState::Empty => Err(NO_DATA_YET.to_string()),
    }
}

const NO_DATA_YET: &str = "No data yet";

#[tauri::command]
pub(crate) async fn get_activity(
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    activity_response(state.host.activity())
}

/// Both parts of the host's state as one coherent pair, with their stamps.
#[tauri::command]
pub(crate) async fn get_runtime_state(
    state: tauri::State<'_, AppState>,
) -> Result<RuntimeState, String> {
    Ok(state.host.runtime_state())
}

#[tauri::command]
pub(crate) async fn get_providers(
    state: tauri::State<'_, AppState>,
) -> Result<ProvidersResponse, String> {
    Ok(state.host.providers_response())
}

/// Starts the collection and answers at once; `get_activity` shows it filling up.
#[tauri::command]
pub(crate) async fn collect(
    state: tauri::State<'_, AppState>,
    request: CollectRequest,
) -> Result<CollectAnswer, String> {
    let options = resolve_collect_options(&request).map_err(str::to_string)?;
    let answered = state.host.collect(options);
    let activity = ShownActivity::of(answered.value).ok_or_else(|| NO_DATA_YET.to_string())?;
    Ok(CollectAnswer {
        activity,
        stamp: answered.stamp,
    })
}

/// Runs a connection's startup check again and collects its kind again for the range on
/// screen; answers with the sources, this one checking, and their stamp.
#[tauri::command]
pub(crate) async fn recheck_connection(
    state: tauri::State<'_, AppState>,
    request: ConnectionId,
) -> Result<RecheckAnswer, String> {
    let answered = state
        .host
        .recheck(&request)
        .ok_or_else(|| format!("No source {request}"))?;
    Ok(RecheckAnswer {
        providers: answered.value,
        stamp: answered.stamp,
    })
}

/// Async, so reading the file and listing WSL distributions (the registry) never holds the
/// window's thread, which runs the commands that are not.
#[tauri::command]
pub(crate) async fn get_provider_config() -> Result<ProviderConfigSnapshotDto, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let connections = match load_config()? {
            Some(file) => shown_connections(&file)?,
            None => Vec::new(),
        };
        Ok(build_snapshot(&connections, &current_os()))
    })
    .await
    .map_err(|err| format!("Reading settings failed: {err}"))?
}

#[tauri::command]
pub(crate) async fn save_provider_config(
    state: tauri::State<'_, AppState>,
    request: ProviderConfigSaveRequestDto,
) -> Result<ProviderConfigSaveResponseDto, String> {
    Ok(
        update_config(&state, move |file| apply_save(file, &request))
            .await?
            .1,
    )
}

#[tauri::command]
pub(crate) async fn add_connection(
    state: tauri::State<'_, AppState>,
    request: AddConnectionRequestDto,
) -> Result<AddConnectionResponseDto, String> {
    let (created, saved) = update_config(&state, move |file| apply_add(file, &request)).await?;
    diagnostics::info("settings", format!("Added {created}"));
    Ok(AddConnectionResponseDto { created, saved })
}

#[tauri::command]
pub(crate) async fn remove_connection(
    state: tauri::State<'_, AppState>,
    request: ConnectionId,
) -> Result<ProviderConfigSaveResponseDto, String> {
    let removed = request.clone();
    let (_, saved) = update_config(&state, move |file| {
        file.remove(&request).map_err(|err| format!("{err:#}"))
    })
    .await?;
    diagnostics::info("settings", format!("Removed {removed}"));
    Ok(saved)
}

/// Edits `config.toml` (see `Host::edit`) and installs it: the checks of every connection run
/// again in the background, and only the kinds the edit affects are collected again. The
/// response lists the connections as installed, the ones checked again as checking. Answers
/// with what `change` returned and the new settings.
async fn update_config<T: Send + 'static>(
    state: &AppState,
    change: impl FnOnce(&mut ConfigFile) -> Result<T, String> + Send + 'static,
) -> Result<(T, ProviderConfigSaveResponseDto), String> {
    let edit = state
        .host
        .edit(Precondition::None, |file| {
            change(file).map_err(anyhow::Error::msg)
        })
        .await
        .map_err(|err| match err {
            EditError::Io(_) => {
                let message = format!("Saving settings failed: {err}");
                diagnostics::error("settings", &message);
                message
            }
            // The agreed text goes to the pane as it is; the log says what failed.
            EditError::Unprotectable { .. } => {
                let message = err.to_string();
                diagnostics::error("settings", format!("Saving settings failed: {message}"));
                message
            }
            other => other.message(None),
        })?;
    diagnostics::info("settings", "Settings saved, re-running provider checks");

    let connections = edit.connections;
    // Off the async workers: it lists WSL distributions from the registry.
    let snapshot =
        tauri::async_runtime::spawn_blocking(move || build_snapshot(&connections, &current_os()))
            .await
            .map_err(|err| format!("Reading settings failed: {err}"))?;
    Ok((
        edit.value,
        ProviderConfigSaveResponseDto {
            snapshot,
            providers_response: edit.providers,
            installed: edit.installed,
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
    write_export(&path, &file.content)
}

/// Writes exactly the file the user confirmed in the dialog, which asked before replacing it.
/// Its name is never changed afterwards: another extension could name a file the user did not
/// choose and was never asked about.
fn write_export(path: &std::path::Path, content: &str) -> Result<(), String> {
    std::fs::write(path, content).map_err(|err| format!("Failed to save export: {err}"))
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
