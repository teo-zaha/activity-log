use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_with::skip_serializing_none;
use ts_rs::TS;

use crate::model::gitlab::gitlab_entry_type::GitLabType;

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct GitLabEntry {
    #[ts(type = "\"gitlab\"")]
    pub source: String,
    #[serde(rename = "type")]
    pub entry_type: GitLabType,
    pub timestamp: String,
    pub title: String,
    pub target_type: String,
    #[ts(optional)]
    pub mr_iid: Option<i32>,
    #[ts(optional)]
    pub branch: Option<String>,
    #[ts(optional)]
    pub commit_count: Option<i32>,
    #[ts(optional)]
    pub note_body: Option<String>,
    #[ts(optional)]
    pub note_file: Option<String>,
    #[ts(optional)]
    pub note_resolved: Option<bool>,
    #[ts(type = "Record<string, unknown>")]
    pub raw: Value,
}
