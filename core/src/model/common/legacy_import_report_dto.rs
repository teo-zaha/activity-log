use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::common::{
    connection_id::ConnectionId, legacy_file_dto::LegacyFileDto,
    legacy_import_skip_dto::LegacyImportSkipDto, legacy_store_wait::LegacyStoreWait,
};

/// What the one import of Activity Log 0.0.7's settings did at this start. `message` says it
/// for people, as the Logs pane and the CLI do; the kind and the lists are what code reads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, tag = "kind", rename_all = "camelCase")]
pub enum LegacyImportReportDto {
    /// The sources were written to `config.toml`.
    #[serde(rename_all = "camelCase")]
    #[ts(rename_all = "camelCase")]
    Imported {
        file: LegacyFileDto,
        /// The other file found, which the import did not read.
        #[ts(optional)]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        unused: Option<LegacyFileDto>,
        imported: Vec<ConnectionId>,
        skipped: Vec<LegacyImportSkipDto>,
        message: String,
    },
    /// Nothing was written: the settings bring a token and no store gave a key. A later start
    /// imports them.
    Postponed {
        file: LegacyFileDto,
        store: LegacyStoreWait,
        message: String,
    },
    /// The file could not be read as 0.0.7 read it; nothing was written.
    Unreadable {
        file: LegacyFileDto,
        message: String,
    },
    /// Writing `config.toml` failed (a busy lock, a location that keeps no file private, a
    /// sealed value that did not check); nothing was written, and a later start tries again.
    Failed {
        file: LegacyFileDto,
        message: String,
    },
}
