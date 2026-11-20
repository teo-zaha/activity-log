use activity_log_core::{
    diagnostics,
    export::render_export_request,
    model::{
        AddConnectionRequestDto, AddConnectionResponseDto, BackendError, BackendErrorKind,
        CollectAnswer, CollectRequest, ConnectionId, ExportOutcome, ExportRequest, LogsResponse,
        ProviderConfigSaveRequestDto, ProviderConfigSaveResponseDto, ProviderConfigSnapshotDto,
        RecheckAnswer, RuntimeState,
    },
    provider::current_os,
    runtime::{answer_collect, answer_recheck},
    settings::{ConfigEditError, ConfigFile, Precondition},
};
use rfd::FileDialog;
use tauri::Url;
use tauri_plugin_opener::OpenerExt;

use crate::{
    app_state::AppState,
    provider_config::{
        apply_add, apply_save, build_snapshot, load_config, save_changes, shown_connections,
    },
};

#[cfg(test)]
mod tests;

/// Both parts of the host's state as one coherent pair, with their stamps.
#[tauri::command]
pub(crate) async fn get_runtime_state(
    state: tauri::State<'_, AppState>,
) -> Result<RuntimeState, BackendError> {
    Ok(state.host.runtime_state())
}

/// Starts the collection and answers at once; `get_runtime_state` shows it filling up.
#[tauri::command]
pub(crate) async fn collect(
    state: tauri::State<'_, AppState>,
    request: CollectRequest,
) -> Result<CollectAnswer, BackendError> {
    answer_collect(&state.host, &request)
}

/// Runs a connection's startup check again and collects its kind again for the range on
/// screen; answers with the sources, this one checking, and their stamp.
#[tauri::command]
pub(crate) async fn recheck_connection(
    state: tauri::State<'_, AppState>,
    request: ConnectionId,
) -> Result<RecheckAnswer, BackendError> {
    answer_recheck(&state.host, &request)
}

/// Async, so reading the file and listing WSL distributions (the registry) never holds the
/// window's thread, which runs the commands that are not.
#[tauri::command]
pub(crate) async fn get_provider_config() -> Result<ProviderConfigSnapshotDto, BackendError> {
    tauri::async_runtime::spawn_blocking(|| {
        let connections = match load_config()? {
            Some(file) => shown_connections(&file),
            None => Vec::new(),
        };
        Ok(build_snapshot(&connections, &current_os()))
    })
    .await
    .map_err(|err| BackendError::internal(format!("Reading settings failed: {err}")))?
}

#[tauri::command]
pub(crate) async fn save_provider_config(
    state: tauri::State<'_, AppState>,
    request: ProviderConfigSaveRequestDto,
) -> Result<ProviderConfigSaveResponseDto, BackendError> {
    let changes = save_changes(&request)?;
    Ok(
        update_config(&state, move |file| apply_save(file, &changes))
            .await?
            .1,
    )
}

#[tauri::command]
pub(crate) async fn add_connection(
    state: tauri::State<'_, AppState>,
    request: AddConnectionRequestDto,
) -> Result<AddConnectionResponseDto, BackendError> {
    let (created, saved) = update_config(&state, move |file| apply_add(file, &request)).await?;
    diagnostics::info("settings", format!("Added {created}"));
    Ok(AddConnectionResponseDto { created, saved })
}

#[tauri::command]
pub(crate) async fn remove_connection(
    state: tauri::State<'_, AppState>,
    request: ConnectionId,
) -> Result<ProviderConfigSaveResponseDto, BackendError> {
    let removed = request.clone();
    let (_, saved) = update_config(&state, move |file| file.remove(&request)).await?;
    diagnostics::info("settings", format!("Removed {removed}"));
    Ok(saved)
}

/// Edits `config.toml` (see `Host::edit`) and installs it: the checks of every connection run
/// again in the background, and only the kinds the edit affects are collected again. The
/// response lists the connections as installed, the ones checked again as checking. Answers
/// with what `change` returned and the new settings. A failure of the app or of the location
/// is logged too; a refused or conflicting change is only answered.
async fn update_config<T: Send + 'static>(
    state: &AppState,
    change: impl FnOnce(&mut ConfigFile) -> Result<T, ConfigEditError> + Send + 'static,
) -> Result<(T, ProviderConfigSaveResponseDto), BackendError> {
    let edit = state
        .host
        .edit(Precondition::None, change)
        .await
        .map_err(|err| {
            let err = BackendError::from(err);
            if matches!(
                err.kind,
                BackendErrorKind::Internal | BackendErrorKind::Unprotectable
            ) {
                diagnostics::error("settings", &err.message);
            }
            err
        })?;
    match edit.installed {
        Some(_) => diagnostics::info("settings", "Settings saved, re-running provider checks"),
        // Written, but the app quit before it installed them: no check runs for them now.
        None => diagnostics::info("settings", "Settings saved; they apply at the next start"),
    }

    let connections = edit.connections;
    // Off the async workers: it lists WSL distributions from the registry.
    let snapshot =
        tauri::async_runtime::spawn_blocking(move || build_snapshot(&connections, &current_os()))
            .await
            .map_err(|err| BackendError::internal(format!("Reading settings failed: {err}")))?;
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
pub(crate) fn save_export(request: ExportRequest) -> Result<ExportOutcome, BackendError> {
    let file = render_export_request(&request)?;
    let extension = request.format.extension();
    let selected_path = FileDialog::new()
        .set_title("Export activity log")
        .set_file_name(&file.file_name)
        .add_filter(request.format.label(), &[extension])
        .save_file();

    let Some(path) = selected_path else {
        return Ok(ExportOutcome::Cancelled);
    };
    write_export(&path, &file.content)?;
    Ok(ExportOutcome::Saved)
}

/// Writes exactly the file the user confirmed in the dialog, which asked before replacing it.
/// Its name is never changed afterwards: another extension could name a file the user did not
/// choose and was never asked about.
fn write_export(path: &std::path::Path, content: &str) -> Result<(), BackendError> {
    std::fs::write(path, content)
        .map_err(|err| BackendError::internal(format!("Failed to save export: {err}")))
}

/// Opens a link from an event in the default browser. Descriptions come from whoever sent the
/// invitation, so only web links are opened, never files or other apps' schemes.
#[tauri::command]
pub(crate) fn open_url(app: tauri::AppHandle, request: String) -> Result<(), BackendError> {
    let url = web_url(&request)?;
    app.opener()
        .open_url(url.as_str(), None::<&str>)
        .map_err(|err| {
            let message = format!("Opening a link failed: {err}");
            diagnostics::warn("desktop", &message);
            BackendError::internal(message)
        })
}

fn web_url(raw: &str) -> Result<Url, BackendError> {
    let url =
        Url::parse(raw.trim()).map_err(|_| BackendError::invalid_request("Not a valid link"))?;
    match url.scheme() {
        "https" | "http" if url.host_str().is_some_and(|host| !host.is_empty()) => Ok(url),
        "https" | "http" => Err(BackendError::invalid_request("The link has no host")),
        other => Err(BackendError::invalid_request(format!(
            "Only web links can be opened, not {other}: links"
        ))),
    }
}
