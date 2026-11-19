use serde::Deserialize;

use super::jira_history_item::JiraHistoryItem;

#[derive(Debug, Deserialize)]
pub(crate) struct JiraHistory {
    /// The changelog entry's id; Jira sends it as text.
    #[serde(default)]
    pub id: Option<String>,
    pub created: String,
    #[serde(default)]
    pub items: Vec<JiraHistoryItem>,
}
