use serde::Deserialize;

use super::jira_history_item::JiraHistoryItem;

#[derive(Debug, Deserialize)]
pub(crate) struct JiraHistory {
    pub created: String,
    #[serde(default)]
    pub items: Vec<JiraHistoryItem>,
}
