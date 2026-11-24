use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{IssueRef, PullRequestRef};

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, tag = "kind", rename_all = "camelCase")]
pub enum Item {
    PullRequest(PullRequestRef),
    Issue(IssueRef),
}
