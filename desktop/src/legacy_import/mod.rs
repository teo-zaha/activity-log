//! Activity Log 0.0.7's settings, imported once before the host first reads `config.toml`
//! (`core::settings::legacy_import`), and what the import did, held until the dashboard asks
//! for it once with `take_legacy_import_report`. The start asks the secure store without a
//! dialog, so a store that needs one (a locked keychain, a key another build made) shows no
//! dialog at every start: the import then waits, and `import_legacy_settings`, which the
//! notice's Import runs, asks with one.

use std::sync::{Mutex, PoisonError};

use activity_log_core::{
    model::{BackendError, LegacyImportReportDto},
    secrets::{FrontEnd, Interaction, Secrets},
    settings::legacy_import::{Place, import_once},
};

use crate::app_state::AppState;

#[cfg(test)]
mod tests;

/// The import's report until the dashboard takes it.
#[derive(Default)]
pub(crate) struct ImportReport(Mutex<Option<LegacyImportReportDto>>);

impl ImportReport {
    pub(crate) fn hold(&self, report: Option<LegacyImportReportDto>) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = report;
    }

    fn take(&self) -> Option<LegacyImportReportDto> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).take()
    }
}

/// The import, where it is due: on the default config path, before the host exists.
pub(crate) fn import_before_host(secrets: &Secrets) -> Option<LegacyImportReportDto> {
    let place = Place::for_front_end(FrontEnd::Desktop, None)?;
    tauri::async_runtime::block_on(import_once(&place, secrets, Interaction::Never))
}

/// The import again, where it is still due, the store allowed to ask: the user chose it. What
/// it wrote the host installs at once. It never fails: what went wrong is the report's (Tauri's
/// async commands that borrow state answer a `Result`).
#[tauri::command]
pub(crate) async fn import_legacy_settings(
    state: tauri::State<'_, AppState>,
) -> Result<Option<LegacyImportReportDto>, BackendError> {
    let Some(place) = Place::for_front_end(FrontEnd::Desktop, None) else {
        return Ok(None);
    };
    let report = import_once(&place, state.host.secrets(), Interaction::Allowed).await;
    if matches!(report, Some(LegacyImportReportDto::Imported { .. })) {
        state.host.reload_if_changed().await;
    }
    Ok(report)
}

/// What the import at this start did, once; `None` after that, or when nothing was imported.
#[tauri::command]
pub(crate) fn take_legacy_import_report(
    report: tauri::State<'_, ImportReport>,
) -> Option<LegacyImportReportDto> {
    report.take()
}
