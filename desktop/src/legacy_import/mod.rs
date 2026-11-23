//! Activity Log 0.0.7's settings, imported once before the host first reads `config.toml`
//! (`core::settings::legacy_import`), and what the import did, held until the dashboard asks
//! for it once with `take_legacy_import_report`.

use std::sync::{Mutex, PoisonError};

use activity_log_core::{
    model::LegacyImportReportDto,
    secrets::{FrontEnd, Secrets},
    settings::legacy_import::{Place, import_once},
};

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
    tauri::async_runtime::block_on(import_once(&place, secrets))
}

/// What the import at this start did, once; `None` after that, or when nothing was imported.
#[tauri::command]
pub(crate) fn take_legacy_import_report(
    report: tauri::State<'_, ImportReport>,
) -> Option<LegacyImportReportDto> {
    report.take()
}
