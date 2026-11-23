use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::common::{connection_id::ConnectionId, legacy_skip_reason::LegacySkipReason};

/// A value of 0.0.7's settings the import left out, never with the value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct LegacyImportSkipDto {
    /// The connection it was for; `None` for an item no connection was made from.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection: Option<ConnectionId>,
    /// The key as 0.0.7 named it, `GITLAB_TOKEN`.
    pub field: String,
    pub reason: LegacySkipReason,
}
