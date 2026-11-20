use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::common::connection_id::ConnectionId;

/// How many entries one collected connection contributed.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ConnectionCount {
    pub connection: ConnectionId,
    pub count: usize,
}
