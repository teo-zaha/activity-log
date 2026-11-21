//! GitLab's side of change stacks: a project's open merge requests, read through `GitLabApi`
//! and handed to the host-neutral walk as [`OpenChange`]s.

use std::sync::atomic::{AtomicUsize, Ordering};

use futures::{FutureExt, future::BoxFuture};
use reqwest::StatusCode;
use url::Origin;

use crate::{
    collect::change_stack::{
        Budget, ChangeLookup, FailureKind, LookupCause, LookupFailure, OpenChange, OpenPage,
        web_link,
    },
    provider::{
        gitlab::api::{GitLabApi, MergeRequestFilter, MergeRequestRow, PageError, Pager},
        remote_api::Retry,
        remote_error::{ApiError, OAuthRefusal},
    },
};

/// Requests in flight at once for one connection.
const CONCURRENCY: usize = 4;
/// Merge request numbers asked for in one request (`iids[]`): GitLab reads at most 100, and
/// the URL stays short.
const NUMBERS_PER_REQUEST: usize = 50;

/// One connection's open merge requests by project id. Every request is paid for with the
/// budget before it is sent, and none is retried: a busy answer is `Throttled` at once.
pub(crate) struct GitLabChanges<'a> {
    api: &'a GitLabApi,
    origin: Origin,
    /// Rows that were no merge request, or one with a blank title or branch.
    skipped: AtomicUsize,
}

/// How much of a list to read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pages {
    First,
    All,
}

impl<'a> GitLabChanges<'a> {
    pub(crate) fn new(api: &'a GitLabApi) -> Self {
        Self {
            api,
            origin: api.web_origin(),
            skipped: AtomicUsize::new(0),
        }
    }

    /// Rows skipped so far, for one warning once the lookups are done.
    pub(crate) fn skipped(&self) -> usize {
        self.skipped.load(Ordering::SeqCst)
    }

    /// `project`'s open merge requests that `filter` asks for, and whether GitLab named a page
    /// after those read. A row the request did not ask for fails the list: the server ignored
    /// a filter, so nothing it sent can be trusted.
    async fn read(
        &self,
        project: u64,
        filter: MergeRequestFilter<'_>,
        pages: Pages,
        budget: &Budget,
    ) -> Result<(Vec<OpenChange>, bool), LookupFailure> {
        let url = self.api.open_merge_requests(project, filter);
        let mut pager = Pager::new(self.api, url, Retry::Never, budget);
        let mut changes = Vec::new();
        while let Some(page) = pager.next_page::<MergeRequestRow>().await {
            let page = page.map_err(failure_of)?;
            let mut skipped = page.skipped;
            for row in page.rows {
                if !filter.asks_for(project, &row) {
                    return Err(LookupFailure::caused_by(
                        FailureKind::Failed,
                        LookupCause::IgnoredFilter,
                    ));
                }
                match self.change_of(row) {
                    Some(change) => changes.push(change),
                    None => skipped += 1,
                }
            }
            self.skipped.fetch_add(skipped, Ordering::SeqCst);
            if pages == Pages::First {
                break;
            }
        }
        Ok((changes, pager.has_next()))
    }

    async fn read_all(
        &self,
        project: u64,
        filter: MergeRequestFilter<'_>,
        budget: &Budget,
    ) -> Result<Vec<OpenChange>, LookupFailure> {
        self.read(project, filter, Pages::All, budget)
            .await
            .map(|(changes, _)| changes)
    }

    /// The change a row is, its link kept only when it leads to this GitLab's pages.
    fn change_of(&self, row: MergeRequestRow) -> Option<OpenChange> {
        let same_repo = row.source_project_id == Some(row.target_project_id);
        let web_url = row
            .web_url
            .as_deref()
            .and_then(|raw| web_link(raw, &self.origin));
        OpenChange::new(
            row.iid,
            row.title,
            row.source_branch,
            row.target_branch,
            same_repo,
            web_url,
        )
    }

    /// The open merge requests of `project` by one of its branches. A blank branch is never
    /// asked about: GitLab would read it as no filter.
    async fn by_branch(
        &self,
        project: u64,
        filter: MergeRequestFilter<'_>,
        branch: &str,
        budget: &Budget,
    ) -> Result<Vec<OpenChange>, LookupFailure> {
        debug_assert!(!branch.trim().is_empty(), "a blank branch was asked about");
        if branch.trim().is_empty() {
            return Ok(Vec::new());
        }
        self.read_all(project, filter, budget).await
    }
}

impl ChangeLookup<u64> for GitLabChanges<'_> {
    fn concurrency(&self) -> usize {
        CONCURRENCY
    }

    fn open_page<'a>(
        &'a self,
        repo: &'a u64,
        budget: &'a Budget,
    ) -> BoxFuture<'a, Result<OpenPage, LookupFailure>> {
        async move {
            let (changes, more) = self
                .read(*repo, MergeRequestFilter::All, Pages::First, budget)
                .await?;
            Ok(if more {
                OpenPage::Partial(changes)
            } else {
                OpenPage::Complete(changes)
            })
        }
        .boxed()
    }

    fn open_numbers<'a>(
        &'a self,
        repo: &'a u64,
        numbers: &'a [u64],
        budget: &'a Budget,
    ) -> BoxFuture<'a, Result<Vec<OpenChange>, LookupFailure>> {
        async move {
            let mut open = Vec::new();
            for chunk in numbers.chunks(NUMBERS_PER_REQUEST) {
                open.extend(
                    self.read_all(*repo, MergeRequestFilter::Numbers(chunk), budget)
                        .await?,
                );
            }
            Ok(open)
        }
        .boxed()
    }

    fn open_from<'a>(
        &'a self,
        repo: &'a u64,
        branch: &'a str,
        budget: &'a Budget,
    ) -> BoxFuture<'a, Result<Vec<OpenChange>, LookupFailure>> {
        self.by_branch(*repo, MergeRequestFilter::From(branch), branch, budget)
            .boxed()
    }

    fn open_into<'a>(
        &'a self,
        repo: &'a u64,
        branch: &'a str,
        budget: &'a Budget,
    ) -> BoxFuture<'a, Result<Vec<OpenChange>, LookupFailure>> {
        self.by_branch(*repo, MergeRequestFilter::Into(branch), branch, budget)
            .boxed()
    }
}

/// Why a list was not read, as a kind every host shares, with what explains it.
fn failure_of(error: PageError<LookupFailure>) -> LookupFailure {
    match error {
        PageError::Refused(failure) => failure,
        PageError::Repeated => {
            LookupFailure::caused_by(FailureKind::Failed, LookupCause::RepeatedPage)
        }
        PageError::Request(error) => LookupFailure::caused_by(kind_of(&error), error),
    }
}

/// The OAuth error code decides first: it names the token whatever the status. Then the
/// status: GitLab answers a project hidden from the token with 404, and one whose merge
/// requests it may not read with 403.
fn kind_of(error: &ApiError) -> FailureKind {
    match error {
        ApiError::Status {
            refusal: Some(refusal),
            ..
        } => match refusal {
            OAuthRefusal::InvalidToken => FailureKind::TokenInvalid,
            OAuthRefusal::InsufficientScope => FailureKind::ScopeMissing,
            OAuthRefusal::InvalidRequest => FailureKind::Failed,
        },
        ApiError::Status {
            status,
            retry_after,
            refusal: None,
            ..
        } => match *status {
            StatusCode::UNAUTHORIZED => FailureKind::TokenInvalid,
            StatusCode::FORBIDDEN | StatusCode::NOT_FOUND => FailureKind::Unreadable,
            StatusCode::TOO_MANY_REQUESTS => FailureKind::Throttled,
            StatusCode::SERVICE_UNAVAILABLE if retry_after.is_some() => FailureKind::Throttled,
            _ => FailureKind::Failed,
        },
        ApiError::Timeout | ApiError::Unreachable | ApiError::Decode => FailureKind::Failed,
    }
}

#[cfg(test)]
mod tests;
