use std::sync::Arc;

use serde::Serialize;
use ts_rs::TS;

use crate::model::{
    activity::activity_log::ActivityLog,
    common::collection_failed_response::CollectionFailedResponse,
};

/// The collection a host shows: what `GET /api/activity` answers with, and on the wire the `state` of an [`ActivityUpdate`](super::activity_update::ActivityUpdate).
#[derive(Debug, Clone, Serialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, tag = "kind", rename_all = "camelCase")]
pub enum ActivityState {
    /// Nothing was collected yet.
    Empty,
    /// `log.meta.pending` names the connections still to come.
    Running {
        #[ts(as = "ActivityLog")]
        log: Arc<ActivityLog>,
    },
    Ready {
        #[ts(as = "ActivityLog")]
        log: Arc<ActivityLog>,
    },
    Failed {
        failure: CollectionFailedResponse,
    },
}
