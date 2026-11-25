use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{CommentTarget, GitHubReviewState, Item, PullRequestRef};

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
    CommentedOn {
        target: CommentTarget,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        body: Option<String>,
    },
    Committed {
        sha: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        message: Option<String>,
        branch: String,
        #[serde(default, skip_serializing_if = "Option::is_none", rename = "webUrl")]
        #[ts(optional, rename = "webUrl")]
        web_url: Option<String>,
    },
}

impl GitHubAction {
    pub(crate) fn pull_request_mut(&mut self) -> Option<&mut PullRequestRef> {
        match self {
            Self::Opened {
                target: Item::PullRequest(pull_request),
            }
            | Self::CommentedOn {
                target:
                    CommentTarget::PullRequest(pull_request)
                    | CommentTarget::ReviewLine { pull_request, .. },
                ..
            }
            | Self::Reviewed { pull_request, .. } => Some(pull_request),
            Self::Opened {
                target: Item::Issue(_),
            }
            | Self::CommentedOn {
                target: CommentTarget::Issue(_),
                ..
            }
            | Self::Committed { .. } => None,
        }
    }
}
