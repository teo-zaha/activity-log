use std::sync::Arc;

use serde::Serialize;
use ts_rs::TS;

use crate::model::{
    activity::activity_log::ActivityLog,
    common::collection_failed_response::CollectionFailedResponse,
    runtime::activity_state::ActivityState,
};

/// The activity a command answer shows: the log or the failure, with the host's own kind for it,
/// so a reader never infers the kind from the log. Flattened into the answer, the log's fields
/// stay where `GET /api/activity` has them.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, tag = "kind", rename_all = "camelCase")]
pub enum ShownActivity {
    Running(#[ts(as = "ActivityLog")] Arc<ActivityLog>),
    Ready(#[ts(as = "ActivityLog")] Arc<ActivityLog>),
    Failed(CollectionFailedResponse),
}

impl ShownActivity {
    /// What a command answers for `state`; nothing before the first collection.
    pub fn of(state: ActivityState) -> Option<Self> {
        match state {
            ActivityState::Empty => None,
            ActivityState::Running { log } => Some(Self::Running(log)),
            ActivityState::Ready { log } => Some(Self::Ready(log)),
            ActivityState::Failed { failure } => Some(Self::Failed(failure)),
        }
    }
}
