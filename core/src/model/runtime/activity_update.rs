use std::sync::Arc;

use serde::Serialize;
use ts_rs::TS;

use crate::model::runtime::activity_state::ActivityState;

/// The activity part of a host's state with its stamp (`docs/plans/shared-contracts.md` §5).
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct ActivityUpdate {
    pub epoch: String,
    pub authority: String,
    #[ts(type = "number")]
    pub revision: u64,
    #[ts(as = "ActivityState")]
    pub state: Arc<ActivityState>,
}
