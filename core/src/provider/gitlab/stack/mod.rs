//! GitLab's side of change stacks: a project's open merge requests, read through `GitLabApi`
//! and handed to the host-neutral walk as [`OpenChange`]s, and each merge request event's
//! stack written on its entry ([`annotate`]).

use std::{
    collections::HashMap,
    sync::atomic::{AtomicUsize, Ordering},
};

use futures::{FutureExt, future::BoxFuture};
use reqwest::StatusCode;
use url::Origin;

use crate::{
    collect::change_stack::{
        Budget, ChangeLookup, ChangeRef, FailureKind, LookupCause, LookupFailure, OpenChange,
        OpenPage, change_stacks, web_link,
    },
    diagnostics,
    model::{ActivityEntry, ChangeStack, GitLabEntry},
    provider::{
        gitlab::{
            GitLabConfig,
            api::{GitLabApi, MergeRequestFilter, MergeRequestRow, PageError, Pager},
        },
        provider_context::ProviderContext,
        remote_api::Retry,
        remote_error::{ApiError, OAuthRefusal},
    },
};

/// Writes on every GitLab entry the stack of the merge request it is about, as GitLab has it
/// now, and clears it where there is none: on a push (which folds into a commit), on an event
/// about no open merge request, and on one whose stack could not be read. Every stack is looked
/// up first and then written in one pass without an await, so stopping it leaves each entry as
/// it was or as written.
pub(crate) async fn annotate(entries: &mut [ActivityEntry], context: ProviderContext) {
    let wanted: Vec<_> = entries
        .iter()
        .filter_map(|entry| match entry {
            ActivityEntry::GitLab(entry) => Some((change_of(entry)?, entry.timestamp.utc())),
            _ => None,
        })
        .collect();
    let stacks = if wanted.is_empty() {
        HashMap::new()
    } else {
        look_up(wanted, &context).await
    };
    for entry in entries.iter_mut() {
        if let ActivityEntry::GitLab(entry) = entry {
            entry.change_stack = change_of(entry).and_then(|change| stacks.get(&change).cloned());
        }
    }
}

/// The merge request an entry is about, in its project; none for a push.
fn change_of(entry: &GitLabEntry) -> Option<ChangeRef<u64>> {
    if entry.entry_type.is_push() {
        return None;
    }
    Some(ChangeRef {
        repo: entry.project_id?,
        number: entry.mr_iid?,
    })
}

/// The stacks of `wanted`'s merge requests, within one collection's budget, with what was not
/// found logged.
async fn look_up(
    wanted: Vec<(ChangeRef<u64>, chrono::DateTime<chrono::Utc>)>,
    context: &ProviderContext,
) -> HashMap<ChangeRef<u64>, ChangeStack> {
    let config = match context.config::<GitLabConfig>() {
        Ok(config) => config,
        Err(err) => {
            diagnostics::warn("gitlab", format!("Change stacks skipped: {err:#}"));
            return HashMap::new();
        }
    };
    let api = match GitLabApi::new(config) {
        Ok(api) => api,
        Err(err) => {
            diagnostics::warn(
                "gitlab",
                format!("Change stacks skipped: {}", err.reason(GitLabApi::SERVICE)),
            );
            return HashMap::new();
        }
    };
    let lookup = GitLabChanges::new(&api);
    let budget = Budget::for_collection();
    let report = change_stacks(wanted, &lookup, &budget).await;
    report.log("gitlab", GitLabApi::SERVICE);
    let skipped = lookup.skipped();
    if skipped > 0 {
        diagnostics::warn(
            "gitlab",
            format!(
                "Change stacks: skipped {skipped} {} GitLab sent that could not be read",
                if skipped == 1 {
                    "merge request"
                } else {
                    "merge requests"
                }
            ),
        );
    }
    report.stacks
}

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
