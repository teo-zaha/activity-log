use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::common::source::Source;

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CollectedActivitySummary {
    pub git_count: usize,
    pub gitlab_count: usize,
    pub jira_count: usize,
    pub outlook_count: usize,
    pub system_count: usize,
    pub merged_count: usize,
}

impl CollectedActivitySummary {
    pub fn entry_count_for_source(&self, source: &Source) -> usize {
        match source {
            Source::Git => self.git_count,
            Source::Gitlab => self.gitlab_count,
            Source::Jira => self.jira_count,
            Source::Outlook => self.outlook_count,
            Source::System => self.system_count,
        }
    }
}
