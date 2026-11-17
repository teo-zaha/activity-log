use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::common::connection_id::ConnectionId;

/// RFC 3339 instants: when the log was generated, and the collected range `[start, end)`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ActivityLogMeta {
    pub generated: String,
    pub start: String,
    pub end: String,
    /// Connections still being checked or collected: the log is partial until this is empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(as = "Option<Vec<ConnectionId>>", optional)]
    pub pending: Vec<ConnectionId>,
}
