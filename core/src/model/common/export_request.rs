use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::{activity::activity_entry::ActivityEntry, common::output_format::OutputFormat};

/// Entries to write to a file, as the reader sees them: `[start, end)` as RFC 3339 instants and
/// the IANA time zone (`Europe/Berlin`) that days and times are shown in.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct ExportRequest {
    pub format: OutputFormat,
    pub start: String,
    pub end: String,
    pub time_zone: String,
    pub entries: Vec<ActivityEntry>,
}
