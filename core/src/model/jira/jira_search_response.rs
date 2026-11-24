use serde::Deserialize;

use super::jira_issue::JiraIssue;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JiraSearchResponse {
    pub issues: Vec<JiraIssue>,
    /// Paging as the server reports it; each may be missing, and a page may hold fewer issues
    /// than were asked for.
    #[serde(default)]
    pub start_at: Option<usize>,
    #[serde(default)]
    pub total: Option<usize>,
    #[serde(default)]
    pub is_last: Option<bool>,
}
