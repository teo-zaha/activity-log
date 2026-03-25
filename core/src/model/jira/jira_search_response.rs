use serde::Deserialize;

use super::jira_issue::JiraIssue;

#[derive(Debug, Deserialize)]
pub(crate) struct JiraSearchResponse {
    #[serde(default)]
    pub issues: Vec<JiraIssue>,
}
