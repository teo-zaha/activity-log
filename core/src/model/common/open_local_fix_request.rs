use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{connection_id::ConnectionId, local_fix_target::LocalFixTarget};

/// The exact native action currently offered for a desktop connection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct OpenLocalFixRequest {
    pub connection: ConnectionId,
    pub target: LocalFixTarget,
}
