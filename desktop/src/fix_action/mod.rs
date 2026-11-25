//! Executes the native recovery currently offered by the desktop host.

use activity_log_core::{
    diagnostics,
    model::{BackendError, OpenLocalFixRequest, ProviderFixAction, ProviderInfo},
};
use serde::Deserialize;
use tauri::{
    Runtime, State, WebviewWindow,
    ipc::{InvokeBody, Request},
};

use crate::{app_state::AppState, deep_link::MAIN_WINDOW};

#[cfg(not(target_os = "macos"))]
mod fallback;
#[cfg(target_os = "macos")]
mod macos;

#[cfg(not(target_os = "macos"))]
use fallback as imp;
#[cfg(target_os = "macos")]
use macos as imp;

#[cfg(test)]
mod tests;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Payload {
    request: OpenLocalFixRequest,
}

fn decode(body: &InvokeBody) -> Result<OpenLocalFixRequest, BackendError> {
    if let InvokeBody::Json(body) = body
        && let Ok(payload) = Payload::deserialize(body)
    {
        return Ok(payload.request);
    }
    diagnostics::debug("desktop", "Refused an invalid local recovery request");
    Err(BackendError::invalid_request(
        "Unknown recovery request or target",
    ))
}

fn authorize(
    label: &str,
    request: &OpenLocalFixRequest,
    providers: &[ProviderInfo],
) -> Result<(), BackendError> {
    if label != MAIN_WINDOW {
        return Err(BackendError::invalid_request(
            "Use the main Activity Log window",
        ));
    }
    let action = ProviderFixAction::Local {
        target: request.target,
    };
    if !providers.iter().any(|info| {
        info.connection == request.connection && info.fix_action.as_ref() == Some(&action)
    }) {
        return Err(BackendError::invalid_request(
            "This recovery action is no longer available for the source",
        ));
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn open_local_fix<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    request: Request<'_>,
) -> Result<(), BackendError> {
    let request = decode(request.body())?;
    authorize(
        window.label(),
        &request,
        &state.host.providers_response().providers,
    )?;
    imp::open(request.target).await
}
