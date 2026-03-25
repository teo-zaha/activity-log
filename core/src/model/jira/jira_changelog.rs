use serde::Deserialize;

use super::jira_history::JiraHistory;

#[derive(Debug, Deserialize)]
pub(crate) struct JiraChangelog {
    #[serde(default)]
    pub histories: Vec<JiraHistory>,
}
