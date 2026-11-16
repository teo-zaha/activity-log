use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// RFC 3339 instants: when the log was generated, and the collected range `[start, end)`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ActivityLogMeta {
    pub generated: String,
    pub start: String,
    pub end: String,
}
