use serde::Serialize;
use ts_rs::TS;

use crate::model::runtime::{activity_update::ActivityUpdate, providers_update::ProvidersUpdate};

/// Both parts of a host's state from one moment: what `GET /api/runtime-state` and the
/// desktop's `get_runtime_state` answer with. Both parts carry the same epoch and authority.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct RuntimeState {
    pub activity: ActivityUpdate,
    pub providers: ProvidersUpdate,
}
