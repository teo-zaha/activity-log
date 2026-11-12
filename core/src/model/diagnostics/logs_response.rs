use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::log_entry::LogEntry;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct LogsResponse {
    /// Oldest first.
    pub entries: Vec<LogEntry>,
}
