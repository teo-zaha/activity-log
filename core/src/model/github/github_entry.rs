use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::{ConnectionName, GitHubTag, Timestamp};

use super::GitHubAction;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct GitHubEntry {
    #[ts(type = "\"github\"")]
    pub source: GitHubTag,
    #[ts(as = "String")]
    pub connection: ConnectionName,
    pub id: String,
    #[ts(as = "String")]
    pub timestamp: Timestamp,
    pub repository: String,
    #[serde(flatten)]
    #[ts(flatten)]
    pub action: GitHubAction,
}
