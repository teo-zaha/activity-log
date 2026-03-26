use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::jira::jira_entry_type::JiraEntryType;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct JiraEntry {
    #[ts(type = "\"jira\"")]
    pub source: String,
    #[serde(rename = "type")]
    #[ts(rename = "type")]
    pub entry_type: JiraEntryType,
    pub timestamp: String,
    pub key: String,
    pub title: String,
    pub status: String,
    pub issue_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub from_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub to_status: Option<String>,
}
