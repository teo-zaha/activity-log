use serde::Deserialize;

use super::{jira_changelog::JiraChangelog, jira_issue_fields::JiraIssueFields};

#[derive(Debug, Deserialize)]
pub(crate) struct JiraIssue {
    pub key: String,
    pub fields: JiraIssueFields,
    #[serde(default)]
    pub changelog: Option<JiraChangelog>,
}
