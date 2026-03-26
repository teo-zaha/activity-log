use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;
use ts_rs::TS;

use crate::model::jira::jira_entry_type::JiraEntryType;

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct JiraEntry {
    #[ts(type = "\"jira\"")]
    pub source: String,
    #[serde(rename = "type")]
    pub entry_type: JiraEntryType,
    pub timestamp: String,
    pub key: String,
    pub title: String,
    pub status: String,
    pub issue_type: String,
    #[ts(optional)]
    pub from_status: Option<String>,
    #[ts(optional)]
    pub to_status: Option<String>,
}
