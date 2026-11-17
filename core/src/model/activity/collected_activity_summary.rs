use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::common::connection_id::ConnectionId;

/// How many entries each collected connection contributed.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct CollectedActivitySummary {
    pub connections: Vec<ConnectionCount>,
    /// Git commits that absorbed their GitLab push event.
    pub merged_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ConnectionCount {
    pub connection: ConnectionId,
    pub count: usize,
}

impl CollectedActivitySummary {
    pub fn entry_count_for(&self, connection: &ConnectionId) -> usize {
        self.connections
            .iter()
            .find(|count| count.connection == *connection)
            .map_or(0, |count| count.count)
    }
}
