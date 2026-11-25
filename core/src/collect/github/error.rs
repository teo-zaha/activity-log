#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CollectionError {
    ReviewPullRequestMismatch,
    ReviewAuthorMismatch,
    CommentOrder,
    CommentReviewMismatch,
    StateChangeKindMismatch,
    CloseTimelineMismatch,
    CloseTimelineUnreadable,
    IncompleteCommitDiscovery,
    CommitRepositoryMismatch,
    CommitRepositoryUnavailable,
    CommitBranchChanged,
}

impl std::fmt::Display for CollectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::ReviewPullRequestMismatch => {
                "GitHub returned reviews for a different pull request"
            }
            Self::ReviewAuthorMismatch => "GitHub returned a review by a different user",
            Self::CommentOrder => "GitHub returned conversation comments out of update order",
            Self::CommentReviewMismatch => "GitHub returned comments for a different review",
            Self::StateChangeKindMismatch => {
                "GitHub returned a different kind of item in a merge or close search"
            }
            Self::CloseTimelineMismatch => "GitHub returned close events for a different item",
            Self::CloseTimelineUnreadable => "GitHub did not return the item's close events",
            Self::IncompleteCommitDiscovery => {
                "GitHub did not list every repository with commits for one day"
            }
            Self::CommitRepositoryMismatch => {
                "GitHub returned commit history for a different repository"
            }
            Self::CommitRepositoryUnavailable => {
                "GitHub did not return the repository's commit history"
            }
            Self::CommitBranchChanged => {
                "GitHub changed the default branch during commit history pagination"
            }
        })
    }
}

impl std::error::Error for CollectionError {}
