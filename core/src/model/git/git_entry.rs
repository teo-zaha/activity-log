use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::{ConnectionName, GitTag, Timestamp, gitlab::gitlab_entry::GitLabEntry};

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct GitEntry {
    #[ts(type = "\"git\"")]
    pub source: GitTag,
    /// The connection within its kind, such as `oss`; stamped by the runtime before the entry is
    /// cached or sent.
    #[ts(as = "String")]
    pub connection: ConnectionName,
    /// The source's own id for the entry (a commit hash, an event id), unique within its
    /// connection: `source`, `connection` and `id` identify an entry.
    pub id: String,
    #[ts(as = "String")]
    pub timestamp: Timestamp,
    pub repo: String,
    pub hash: String,
    pub message: String,
    pub author: String,
    /// The branch HEAD names, on the commit HEAD points at; absent on every other commit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "gitlabEvent")]
    #[ts(rename = "gitlabEvent", optional)]
    pub gitlab_event: Option<Box<GitLabEntry>>,
}

impl GitEntry {
    pub fn new(
        repo: String,
        hash: String,
        message: String,
        author: String,
        branch: Option<String>,
        timestamp: Timestamp,
    ) -> Self {
        Self {
            source: GitTag,
            connection: crate::model::ConnectionName::unstamped(),
            id: hash.clone(),
            timestamp,
            repo,
            hash,
            message,
            author,
            branch,
            gitlab_event: None,
        }
    }
}
