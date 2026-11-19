use std::sync::Arc;

use crate::model::{
    activity::activity_log::ActivityLog,
    common::collection_failed_response::CollectionFailedResponse,
};

/// The collection a host shows: what `GET /api/activity` and the desktop's `get_activity`
/// answer with.
#[derive(Debug, Clone)]
pub enum ActivityState {
    /// Nothing was collected yet.
    Empty,
    /// `log.meta.pending` names the connections still to come.
    Running {
        log: Arc<ActivityLog>,
    },
    Ready {
        log: Arc<ActivityLog>,
    },
    Failed {
        failure: CollectionFailedResponse,
    },
}
