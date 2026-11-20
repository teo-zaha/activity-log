use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::common::connection_id::ConnectionId;

/// Removes one connection; `{ source, name }` alone still reads, as before.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct RemoveConnectionRequestDto {
    #[serde(flatten)]
    #[ts(flatten)]
    pub connection: ConnectionId,
    /// The snapshot `revision` the pane showed: removing a connection another program replaced
    /// under the same name is a conflict. Without it, no precondition.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub based_on: Option<String>,
}
