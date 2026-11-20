use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::{ConnectionName, JiraTag, Timestamp, jira::jira_entry_type::JiraEntryType};

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct JiraEntry {
    #[ts(type = "\"jira\"")]
    pub source: JiraTag,
    /// The connection within its kind, such as `oss`; stamped by the runtime before the entry is
    /// cached or sent.
    #[ts(as = "String")]
    pub connection: ConnectionName,
    /// The source's own id for the entry (a commit hash, an event id), unique within its
    /// connection: `source`, `connection` and `id` identify an entry.
    pub id: String,
    #[serde(rename = "type")]
    #[ts(rename = "type")]
    pub entry_type: JiraEntryType,
    #[ts(as = "String")]
    pub timestamp: Timestamp,
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
