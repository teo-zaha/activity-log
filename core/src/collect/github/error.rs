#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CollectionError {
    RepeatedPage,
    MissingNextCursor,
    ReviewPullRequestMismatch,
    ReviewAuthorMismatch,
}

impl std::fmt::Display for CollectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::RepeatedPage => "GitHub repeated a page of results",
            Self::MissingNextCursor => "GitHub did not name the next page of results",
            Self::ReviewPullRequestMismatch => {
                "GitHub returned reviews for a different pull request"
            }
            Self::ReviewAuthorMismatch => "GitHub returned a review by a different user",
        })
    }
}

impl std::error::Error for CollectionError {}
