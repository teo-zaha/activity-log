use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Instants as RFC 3339 strings; `start` is inclusive, `end` exclusive.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CollectRequest {
    pub start: String,
    pub end: String,
}
