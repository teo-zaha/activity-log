use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::common::collect_options::CollectOptions;

/// The last collection failed as a whole. It carries its range, so the dashboard can still show
/// which days were asked for. A running collection is an `ActivityLog` with `meta.pending`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CollectionFailedResponse {
    pub start: String,
    pub end: String,
    pub error: String,
}

impl CollectionFailedResponse {
    pub fn new(options: &CollectOptions, error: impl Into<String>) -> Self {
        Self {
            start: options.start.to_rfc3339(),
            end: options.end.to_rfc3339(),
            error: error.into(),
        }
    }
}
