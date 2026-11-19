use serde::Deserialize;

use super::jira_history::JiraHistory;

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JiraChangelog {
    #[serde(default)]
    pub histories: Vec<JiraHistory>,
    /// Where the embedded page of histories starts and how many there are in all.
    #[serde(default)]
    pub start_at: Option<usize>,
    #[serde(default)]
    pub total: Option<usize>,
}

impl JiraChangelog {
    /// The embedded histories are only part of the issue's changelog.
    pub(crate) fn is_partial(&self) -> bool {
        self.start_at.is_some_and(|start| start > 0)
            || self.total.is_some_and(|total| total > self.histories.len())
    }
}
