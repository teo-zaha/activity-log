use crate::model::activity::{
    activity_log::ActivityLog, collected_activity_summary::CollectedActivitySummary,
};

#[derive(Debug, Clone)]
pub struct CollectedActivity {
    pub summary: CollectedActivitySummary,
    pub log: ActivityLog,
}
