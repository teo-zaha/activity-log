use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::activity::{
    activity_entry::ActivityEntry, activity_log_meta::ActivityLogMeta,
    collected_activity_summary::CollectedActivitySummary,
};

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ActivityLog {
    pub meta: ActivityLogMeta,
    pub summary: CollectedActivitySummary,
    pub entries: Vec<ActivityEntry>,
}
