use activity_log_core::{
    diagnostics,
    export::render_export_request,
    model::{
        AddConnectionRequestDto, AddConnectionResponseDto, BackendError, BackendErrorKind,
        CollectAnswer, CollectRequest, ConnectionId, ExportOutcome, ExportRequest, LogsResponse,
        ProviderConfigCheckResponseDto, ProviderConfigSaveRequestDto,
        ProviderConfigSaveResponseDto, ProviderConfigSnapshotDto, RecheckAnswer,
        RemoveConnectionRequestDto, RuntimeState,
    },
    provider::current_os,
    runtime::{answer_collect, answer_recheck},
    secrets::{Interaction, Sealer},
    settings::{
        ConfigEditError, ConfigFile, Conflict, EditError, Precondition,
        view::{apply_add, apply_save, build_snapshot, check_save, save_changes, touched_fields},
    },
};
use std::path::Path;

use rfd::FileDialog;
use tauri::Url;
use tauri_plugin_opener::OpenerExt;

use crate::{
    app_state::AppState,
    provider_config::{load_config, shown_connections},
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
pub(crate) async fn get_provider_config(
    state: tauri::State<'_, AppState>,
) -> Result<ProviderConfigSnapshotDto, BackendError> {
    let path = state.host.config_path().map(Path::to_path_buf);
    let file = tauri::async_runtime::spawn_blocking(move || load_config(path.as_deref()))
        .await
        .map_err(|err| BackendError::internal(format!("Reading settings failed: {err}")))??;
    snapshot_of(&state, file.unwrap_or_else(ConfigFile::empty)).await
}

/// The issues a save of `request` would be refused for, without saving it: values that do not
/// fit, and fields another program changed since its `basedOn` snapshot (compared with the file
/// as it is now, read without the lock). It cannot promise the save succeeds; the file may
/// change again, and the save's own precondition decides.
#[tauri::command]
pub(crate) async fn check_provider_config(
    state: tauri::State<'_, AppState>,
    request: ProviderConfigSaveRequestDto,
) -> Result<ProviderConfigCheckResponseDto, BackendError> {
    let base = state.snapshots.base(
        &request.based_on,
        &request.connection,
        &touched_fields(&request),
    );
    let path = state.host.config_path().map(Path::to_path_buf);
    let current = tauri::async_runtime::spawn_blocking(move || load_config(path.as_deref()))
        .await
        .map_err(|err| BackendError::internal(format!("Reading settings failed: {err}")))??;
    Ok(check_save(&request, base, current.as_ref()))
}

/// Saves one connection's edits if what they were based on is still so under the config lock;
/// another program's change to a touched or account field is a `conflict`, and nothing is
/// saved.
#[tauri::command]
pub(crate) async fn save_provider_config(
    state: tauri::State<'_, AppState>,
    request: ProviderConfigSaveRequestDto,
) -> Result<ProviderConfigSaveResponseDto, BackendError> {
    let changes = save_changes(&request)?;
    let precondition = state
        .snapshots
        .base(
            &request.based_on,
            &request.connection,
            &touched_fields(&request),
        )
        .precondition()
        .map_err(|conflict| unknown_base_refusal(&state, conflict))?;
    let sealing = match changes.brings_secret() {
        true => Some(sealer(&state).await?),
        false => None,
    };
    Ok(update_config(&state, precondition, sealing, move |file| {
        apply_save(file, &changes)
    })
    .await?
    .1)
}

#[tauri::command]
pub(crate) async fn add_connection(
    state: tauri::State<'_, AppState>,
    request: AddConnectionRequestDto,
) -> Result<AddConnectionResponseDto, BackendError> {
    let (created, saved) = update_config(&state, Precondition::None, None, move |file| {
        apply_add(file, &request)
    })
    .await?;
    diagnostics::info("settings", format!("Added {created}"));
    Ok(AddConnectionResponseDto { created, saved })
}

/// Removes a connection if it is still the one its `basedOn` snapshot showed: one another
/// program replaced under the same name, or changed to another account, is a `conflict`.
#[tauri::command]
pub(crate) async fn remove_connection(
    state: tauri::State<'_, AppState>,
    request: RemoveConnectionRequestDto,
) -> Result<ProviderConfigSaveResponseDto, BackendError> {
    let RemoveConnectionRequestDto {
        connection,
        based_on,
    } = request;
    let precondition = state
        .snapshots
        .base(&based_on, &connection, &[])
        .precondition()
        .map_err(|conflict| unknown_base_refusal(&state, conflict))?;
    let removed = connection.clone();
    let (_, saved) = update_config(&state, precondition, None, move |file| {
        file.remove(&connection)
    })
    .await?;
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
    precondition: Precondition,
    sealing: Option<Sealer>,
    change: impl FnOnce(&mut ConfigFile) -> Result<T, ConfigEditError> + Send + 'static,
) -> Result<(T, ProviderConfigSaveResponseDto), BackendError> {
    let edit = state
        .host
        .edit(precondition, sealing, change)
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

    let snapshot = snapshot_of(state, edit.file).await?;
    Ok((
        edit.value,
        ProviderConfigSaveResponseDto {
            snapshot,
            providers_response: edit.providers,
            installed: edit.installed,
        },
    ))
}

/// A save or removal whose base this app no longer holds: refused at once, the connection named
/// by the label it shows now (its base, which would say the label it had, is gone).
fn unknown_base_refusal(state: &AppState, mut conflict: Conflict) -> BackendError {
    if conflict.label.is_none() {
        conflict.label = state
            .host
            .providers_response()
            .providers
            .into_iter()
            .find(|info| info.connection == conflict.connection)
            .map(|info| info.label);
    }
    BackendError::from(EditError::Conflict(conflict))
}

/// The snapshot of `file` with the revision a save based on it names; built off the async
/// workers, since it lists WSL distributions from the registry.
async fn snapshot_of(
    state: &AppState,
    file: ConfigFile,
) -> Result<ProviderConfigSnapshotDto, BackendError> {
    let connections = shown_connections(&file);
    let revision = state.snapshots.register(file);
    let secrets = state.host.secrets().clone();
    tauri::async_runtime::spawn_blocking(move || {
        build_snapshot(&connections, &current_os(), revision, &secrets)
    })
    .await
    .map_err(|err| BackendError::internal(format!("Reading settings failed: {err}")))
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

/// Renders the entries the dashboard shows and saves them where the user picks. Async, so the
/// render, the dialog and the write never hold the window's thread; rfd puts the dialog on the
/// main thread itself where the OS needs it (macOS). It renders first, so a request that does
/// not render opens no dialog. One export at a time: another while its dialog is open is
/// refused as `busy`.
#[tauri::command]
pub(crate) async fn save_export(
    exports: tauri::State<'_, ExportTurn>,
    request: ExportRequest,
) -> Result<ExportOutcome, BackendError> {
    let _turn = exports.take()?;
    let format = request.format;
    let file =
        blocking(move || render_export_request(request).map_err(BackendError::from)).await??;
    let name = file.file_name.clone();
    let chosen = blocking(move || {
        FileDialog::new()
            .set_title("Export activity log")
            .set_file_name(&name)
            .add_filter(format.label(), &[format.extension()])
            .save_file()
    })
    .await?;
    let Some(path) = chosen else {
        return Ok(ExportOutcome::Cancelled);
    };
    blocking(move || write_export(&path, &file.content)).await??;
    Ok(ExportOutcome::Saved)
}

/// Whether an export is under way; `take` claims the turn until its guard drops.
#[derive(Default)]
pub(crate) struct ExportTurn(std::sync::atomic::AtomicBool);

impl ExportTurn {
    pub(crate) fn take(&self) -> Result<ExportGuard<'_>, BackendError> {
        use std::sync::atomic::Ordering;
        if self.0.swap(true, Ordering::AcqRel) {
            return Err(BackendError::new(
                BackendErrorKind::Busy,
                "An export is already open",
            ));
        }
        Ok(ExportGuard(&self.0))
    }
}

pub(crate) struct ExportGuard<'a>(&'a std::sync::atomic::AtomicBool);

impl Drop for ExportGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, std::sync::atomic::Ordering::Release);
    }
}

/// `work` on a blocking thread; a panic in it is an internal failure.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, BackendError> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|err| BackendError::internal(format!("Exporting failed: {err}")))
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
pub(crate) fn open_url<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    request: String,
) -> Result<(), BackendError> {
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

/// The key a save that brings a secret seals it with, read from the OS store before the file
/// is locked (a dialog can take as long as the user does). The desktop app seals only there:
/// no store is `secretStoreUnavailable`, a refusal `secretStoreRefused`, the draft kept.
async fn sealer(state: &AppState) -> Result<Sealer, BackendError> {
    let path = state.host.config_path().map(Path::to_path_buf);
    let current = tauri::async_runtime::spawn_blocking(move || load_config(path.as_deref()))
        .await
        .ok()
        .and_then(Result::ok)
        .flatten();
    state
        .host
        .secrets()
        .sealer(Interaction::Allowed, |store| {
            current.as_ref().map_or(0, |file| file.sealed_in(store))
        })
        .await
        .map_err(|err| BackendError::from(EditError::SecretStore(err)))
}
