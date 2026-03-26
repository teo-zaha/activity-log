use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;
use ts_rs::TS;

use crate::model::gitlab::gitlab_entry::GitLabEntry;

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct GitEntry {
    #[ts(type = "\"git\"")]
    pub source: String,
    pub timestamp: String,
    pub repo: String,
    pub hash: String,
    pub message: String,
    pub author: String,
    pub branch: String,
    #[serde(rename = "gitlabEvent")]
    #[ts(rename = "gitlabEvent", optional)]
    pub gitlab_event: Option<GitLabEntry>,
}

impl GitEntry {
    pub fn new(
        repo: String,
        hash: String,
        message: String,
        author: String,
        branch: String,
        timestamp: String,
    ) -> Self {
        Self {
            source: "git".to_string(),
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
