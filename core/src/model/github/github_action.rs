use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{GitHubReviewState, Item, PullRequestRef};

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "camelCase")]
#[ts(export, tag = "type", rename_all = "camelCase")]
pub enum GitHubAction {
    Opened {
        target: Item,
    },
    Reviewed {
        #[serde(rename = "pullRequest")]
        #[ts(rename = "pullRequest")]
        pull_request: PullRequestRef,
        state: GitHubReviewState,
    },
}
