use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::model::gitlab::gitlab_entry_type::GitLabType;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct GitLabEntry {
    #[ts(type = "\"gitlab\"")]
    pub source: String,
    #[serde(rename = "type")]
    #[ts(rename = "type")]
    pub entry_type: GitLabType,
    pub timestamp: String,
    pub title: String,
    pub target_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub mr_iid: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub commit_count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub note_body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub note_file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub note_resolved: Option<bool>,
    #[ts(type = "Record<string, unknown>")]
    pub raw: Value,
}
