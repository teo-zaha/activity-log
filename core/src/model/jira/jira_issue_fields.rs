use serde::Deserialize;

use super::jira_named_field::JiraNamedField;

#[derive(Debug, Deserialize)]
pub(crate) struct JiraIssueFields {
    pub summary: String,
    pub status: JiraNamedField,
    pub issuetype: JiraNamedField,
    pub created: String,
    pub updated: String,
}
