use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{IssueRef, PullRequestRef};

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, tag = "kind", rename_all = "camelCase")]
pub enum CommentTarget {
    PullRequest(PullRequestRef),
    Issue(IssueRef),
    ReviewLine {
        #[serde(rename = "pullRequest")]
        #[ts(rename = "pullRequest")]
        pull_request: PullRequestRef,
        path: String,
    },
}
