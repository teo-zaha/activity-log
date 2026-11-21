//! What a code host answers about its repositories' open changes, in the host's own terms
//! turned neutral: each host implements [`ChangeLookup`] over its repository key.

use futures::future::BoxFuture;

use super::{budget::Budget, web_link::WebLink};
use crate::model::{StackCut, StackedChange};

/// One open change of a repository, as its host listed it: a number, a title and two branches
/// that are not blank (a blank branch would read as no filter where it is asked about).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OpenChange {
    number: u64,
    title: String,
    source_branch: String,
    target_branch: String,
    same_repo: bool,
    web_url: Option<WebLink>,
}

impl OpenChange {
    /// The change, or `None` when its title or a branch is blank: such a row is skipped.
    /// `same_repo` says its source branch is in the repository it targets (not a fork, nor a
    /// deleted fork).
    pub(crate) fn new(
        number: u64,
        title: String,
        source_branch: String,
        target_branch: String,
        same_repo: bool,
        web_url: Option<WebLink>,
    ) -> Option<Self> {
        let blank = |text: &str| text.trim().is_empty();
        (!blank(&title) && !blank(&source_branch) && !blank(&target_branch)).then_some(Self {
            number,
            title,
            source_branch,
            target_branch,
            same_repo,
            web_url,
        })
    }

    pub(crate) fn number(&self) -> u64 {
        self.number
    }

    pub(crate) fn source_branch(&self) -> &str {
        &self.source_branch
    }

    pub(crate) fn target_branch(&self) -> &str {
        &self.target_branch
    }

    pub(crate) fn same_repo(&self) -> bool {
        self.same_repo
    }

    pub(crate) fn stacked(&self) -> StackedChange {
        StackedChange {
            number: self.number,
            title: self.title.clone(),
            web_url: self.web_url.as_ref().map(|link| link.as_str().to_string()),
        }
    }
}

/// A change by its repository and its number there; changes of different repositories never
/// chain.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct ChangeRef<R> {
    pub repo: R,
    pub number: u64,
}

/// The first page of a repository's open changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OpenPage {
    /// Every open change of the repository.
    Complete(Vec<OpenChange>),
    /// Only some: the repository has more than one page of them.
    Partial(Vec<OpenChange>),
}

/// Why a host could not answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum LookupFailure {
    /// The repository is hidden from this token, or its changes are (404, a project-level 403).
    Unreadable,
    /// The token is invalid or lacks the API scope: stops the connection's lookups.
    TokenRefused,
    /// The host asked to slow down: stops the connection's lookups.
    Throttled,
    /// The request budget or the deadline ran out.
    Limit,
    /// Transport, timeout, 5xx, a body that is no list, a repeated page, an ignored filter.
    Failed,
}

impl LookupFailure {
    /// How a stack's end shows this failure.
    pub(crate) fn cut(self) -> StackCut {
        match self {
            Self::Unreadable | Self::TokenRefused => StackCut::Unreadable,
            Self::Throttled => StackCut::Throttled,
            Self::Limit => StackCut::Limit,
            Self::Failed => StackCut::Failed,
        }
    }
}

/// A code host's open changes, by repository key `R` (GitLab's project id, GitHub's
/// `owner/repo` later). Each request a method sends is paid for with `budget` first
/// (`RequestBudget::try_spend`), and none is retried: the host's busy answer is `Throttled`.
/// The runner bounds each call by the budget's deadline.
pub(crate) trait ChangeLookup<R>: Send + Sync {
    /// How many requests may be in flight at once for one connection.
    fn concurrency(&self) -> usize;

    /// The first page of `repo`'s open changes.
    fn open_page<'a>(
        &'a self,
        repo: &'a R,
        budget: &'a Budget,
    ) -> BoxFuture<'a, Result<OpenPage, LookupFailure>>;

    /// Which of `numbers` are open in `repo`, those that are.
    fn open_numbers<'a>(
        &'a self,
        repo: &'a R,
        numbers: &'a [u64],
        budget: &'a Budget,
    ) -> BoxFuture<'a, Result<Vec<OpenChange>, LookupFailure>>;

    /// `repo`'s open changes whose source branch is `branch`.
    fn open_from<'a>(
        &'a self,
        repo: &'a R,
        branch: &'a str,
        budget: &'a Budget,
    ) -> BoxFuture<'a, Result<Vec<OpenChange>, LookupFailure>>;

    /// `repo`'s open changes that target `branch`.
    fn open_into<'a>(
        &'a self,
        repo: &'a R,
        branch: &'a str,
        budget: &'a Budget,
    ) -> BoxFuture<'a, Result<Vec<OpenChange>, LookupFailure>>;
}

#[cfg(test)]
mod tests;
