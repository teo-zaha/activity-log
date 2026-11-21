use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::{
    ChangeStack, ConnectionName, GitLabTag, Timestamp, gitlab::gitlab_entry_type::GitLabType,
};

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct GitLabEntry {
    #[ts(type = "\"gitlab\"")]
    pub source: GitLabTag,
    /// The connection within its kind, such as `oss`; stamped by the runtime before the entry is
    /// cached or sent.
    #[ts(as = "String")]
    pub connection: ConnectionName,
    /// The source's own id for the entry (a commit hash, an event id), unique within its
    /// connection: `source`, `connection` and `id` identify an entry.
    pub id: String,
    #[serde(rename = "type")]
    #[ts(rename = "type")]
    pub entry_type: GitLabType,
    #[ts(as = "String")]
    pub timestamp: Timestamp,
    /// The title of what the event is about; absent for a push, which has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub title: Option<String>,
    /// What the event is about as GitLab names it (`MergeRequest`, `DiffNote`); absent for a
    /// push.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub target_type: Option<String>,
    /// The project the event is in (`project_id`): a merge request's or a comment's project is
    /// the merge request's target project. Absent when GitLab sends none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(type = "number", optional)]
    pub project_id: Option<u64>,
    /// The merge request the event is about, or the one a comment is on.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(type = "number", optional)]
    pub mr_iid: Option<u64>,
    /// The issue the event is about, or the one a comment is on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(type = "number", optional)]
    pub issue_iid: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(type = "number", optional)]
    pub commit_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub note_body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub note_file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub note_resolved: Option<bool>,
    /// The commit a push (`pushedTo`, `pushedNew`) pushed (`push_data.commit_to`); it folds into
    /// that commit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub pushed_commit: Option<String>,
    /// Where the merge request the event is about stands among its project's open merge
    /// requests now, as of the collection, not on the event's day. Never cached: it is looked
    /// up again on every collection, after the day cache.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub change_stack: Option<ChangeStack>,
}
