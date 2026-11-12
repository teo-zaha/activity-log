use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::log_level::LogLevel;

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct LogEntry {
    /// Monotonic per process, so the UI can track rows across refreshes.
    pub id: u32,
    pub timestamp: String,
    pub level: LogLevel,
    /// Area that emitted the entry, e.g. a provider id, `runtime` or `settings`.
    pub scope: String,
    pub message: String,
}
