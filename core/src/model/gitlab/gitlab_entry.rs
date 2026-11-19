use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::gitlab::gitlab_entry_type::GitLabType;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct GitLabEntry {
    #[ts(type = "\"gitlab\"")]
    pub source: String,
    /// The name of the connection within its kind, such as `oss`; set by the runtime.
    #[serde(default)]
    pub connection: String,
    /// The source's own id for the entry (a commit hash, an event id), unique within its
    /// connection: `source`, `connection` and `id` identify an entry.
    pub id: String,
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
    /// The commit a `pushedTo` event pushed (`push_data.commit_to`); it folds into that commit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub pushed_commit: Option<String>,
}
