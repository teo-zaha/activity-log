use std::collections::HashSet;

use anyhow::Result;
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{
    diagnostics,
    model::{
        CollectOptions, Timestamp,
        github::{GitHubAction, IssueRef, Item},
    },
    provider::{
        github::api::{
            GitHubApi,
            pager::{Page, PageError, Pager},
        },
        remote_error::{ApiError, QueryErrorKind},
    },
};

use super::{
    Collection, CollectionError, ConnectionRun,
    commit_repositories::{RepositoryNode, RepositoryQuery},
    entry, node_refusals, pull_request_ref,
    response::{HostedItem, nonblank},
    split_window, windows,
};

macro_rules! search_query {
    ($operation:literal) => {
        concat!(
            include_str!($operation),
            include_str!("queries/state_change_search_fields.graphql"),
            include_str!("queries/close_event_fields.graphql"),
        )
    };
}

pub(super) const SEARCHES: &str = search_query!("queries/state_changes.graphql");
pub(super) const SEARCH: &str = search_query!("queries/state_change_search.graphql");
pub(super) const MORE_CLOSE_EVENTS: &str = concat!(
    include_str!("queries/more_close_events.graphql"),
    include_str!("queries/close_event_fields.graphql"),
);

const FIRST_SEARCH_LISTS: &[&[&str]] = &[
    &["mergeInvolved", "nodes"],
    &["mergeReviewed", "nodes"],
    &["closePulls", "nodes"],
    &["closeIssues", "nodes"],
];
const SKIPPABLE_REFUSALS: &[QueryErrorKind] = &[QueryErrorKind::Forbidden];

const SEARCH_RESULTS: usize = 1_000;
const SEARCH_PAGE_SIZE: usize = 100;
pub(super) const SEARCH_PAGES: usize = SEARCH_RESULTS / SEARCH_PAGE_SIZE;
pub(super) const SEARCH_REQUESTS_PER_WINDOW: usize = 24;
const CLOSE_PAGE_SIZE: usize = 20;
const CLOSE_PAGES_PER_ITEM: usize = 5;
pub(super) const CLOSE_REQUESTS_PER_WINDOW: usize = 24;

#[derive(Clone, Copy, PartialEq, Eq)]
enum CloseKind {
    PullRequest,
    Issue,
}

#[derive(Clone, Copy)]
enum SearchKind {
    MergeInvolved,
    MergeReviewed,
    Close(CloseKind),
}

impl SearchKind {
    fn query(self, login: &str, window: CollectOptions) -> String {
        let (filters, date) = match self {
            Self::MergeInvolved => (format!("is:pr is:merged involves:{login}"), "merged"),
            Self::MergeReviewed => (format!("is:pr is:merged reviewed-by:{login}"), "merged"),
            Self::Close(CloseKind::PullRequest) => (
                format!("is:pr is:closed is:unmerged involves:{login}"),
                "closed",
            ),
            Self::Close(CloseKind::Issue) => {
                (format!("is:issue is:closed involves:{login}"), "closed")
            }
        };
        // GitHub Search ranges include both ends. Selection uses the exact half-open range.
        let start = window.start.to_rfc3339_opts(SecondsFormat::Secs, true);
        let end = window.end.to_rfc3339_opts(SecondsFormat::Secs, true);
        format!("{filters} sort:created-asc {date}:{start}..{end}")
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SearchBounds {
    since: DateTime<Utc>,
    search_size: usize,
    close_size: usize,
}

impl SearchBounds {
    fn new(since: DateTime<Utc>) -> Self {
        Self {
            since,
            search_size: SEARCH_PAGE_SIZE,
            close_size: CLOSE_PAGE_SIZE,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SearchesVariables {
    merge_involved: String,
    merge_reviewed: String,
    close_pulls: String,
    close_issues: String,
    #[serde(flatten)]
    bounds: SearchBounds,
}

#[derive(Serialize)]
struct SearchVariables<'a> {
    query: &'a str,
    merge: bool,
    after: Option<&'a str>,
    #[serde(flatten)]
    bounds: SearchBounds,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MoreCloseVariables<'a> {
    id: &'a str,
    since: DateTime<Utc>,
    after: &'a str,
    close_size: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Searches {
    merge_involved: SearchPage,
    merge_reviewed: SearchPage,
    close_pulls: SearchPage,
    close_issues: SearchPage,
}

#[derive(Deserialize)]
struct Search {
    merges: Option<SearchPage>,
    closes: Option<SearchPage>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchPage {
    issue_count: usize,
    #[serde(flatten)]
    page: Page,
}

#[derive(Deserialize)]
enum MergeKind {
    PullRequest,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MergeCandidate {
    #[serde(rename = "__typename")]
    _kind: MergeKind,
    #[serde(flatten)]
    item: HostedItem,
    #[serde(deserialize_with = "Option::deserialize")]
    merged_by: Option<Actor>,
    #[serde(deserialize_with = "Option::deserialize")]
    merged_at: Option<Timestamp>,
}

#[derive(Deserialize)]
struct CloseCandidate {
    #[serde(flatten)]
    item: HostedItem,
    #[serde(flatten)]
    timeline: CloseTimeline,
}

#[derive(Deserialize)]
#[serde(tag = "__typename")]
enum CloseTimeline {
    PullRequest {
        #[serde(rename = "pullTimeline")]
        page: Page,
    },
    Issue {
        #[serde(rename = "issueTimeline")]
        page: Page,
    },
}

impl CloseTimeline {
    fn into_parts(self) -> (CloseKind, Page) {
        match self {
            Self::PullRequest { page } => (CloseKind::PullRequest, page),
            Self::Issue { page } => (CloseKind::Issue, page),
        }
    }
}

#[derive(Deserialize)]
struct MoreCloseEvents {
    id: String,
    #[serde(flatten)]
    timeline: CloseTimeline,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CloseEvent {
    #[serde(deserialize_with = "nonblank")]
    id: String,
    created_at: Timestamp,
    actor: Option<Actor>,
}

#[derive(Deserialize)]
#[serde(tag = "__typename")]
enum Actor {
    User {
        id: String,
    },
    #[serde(other)]
    Other,
}

struct Allowance {
    search: usize,
    close: usize,
}

impl Allowance {
    fn new() -> Self {
        Self {
            search: SEARCH_REQUESTS_PER_WINDOW.saturating_sub(1),
            close: CLOSE_REQUESTS_PER_WINDOW,
        }
    }
}

/// Claim one request while leaving enough for the first pages of queued date splits.
fn spend(remaining: &mut usize, reserved: usize) -> bool {
    let Some(left) = remaining.checked_sub(1).filter(|&left| left >= reserved) else {
        return false;
    };
    *remaining = left;
    true
}

#[derive(Default)]
struct StateChanges {
    merged: HashSet<String>,
    close_candidates: HashSet<String>,
    closes: HashSet<String>,
    saturated_searches: usize,
    limited_searches: usize,
    limited_timelines: usize,
    unreadable_timelines: usize,
    refused_candidates: usize,
}

impl StateChanges {
    fn warn(&self) {
        for (count, reason) in [
            (
                self.saturated_searches,
                "GitHub's result limit was reached in merge or close searches",
            ),
            (
                self.limited_searches,
                "The request limit was reached in GitHub merge or close searches",
            ),
            (
                self.limited_timelines,
                "The request limit was reached while reading GitHub close timelines",
            ),
            (
                self.unreadable_timelines,
                "Could not read all GitHub close events. Check repository access",
            ),
        ] {
            if count > 0 {
                diagnostics::warn("github", format!("{reason} ({count} affected)."));
            }
        }
    }
}

impl Collection<'_> {
    pub(super) async fn state_changes(&mut self, run: &ConnectionRun) -> Result<()> {
        let mut scan = StateChanges::default();
        for window in windows(self.options) {
            let mut allowance = Allowance::new();
            let pulls = SearchKind::Close(CloseKind::PullRequest);
            let issues = SearchKind::Close(CloseKind::Issue);
            let answer = self
                .api
                .graphql_answer(
                    SEARCHES,
                    &SearchesVariables {
                        merge_involved: SearchKind::MergeInvolved.query(&self.login, window),
                        merge_reviewed: SearchKind::MergeReviewed.query(&self.login, window),
                        close_pulls: pulls.query(&self.login, window),
                        close_issues: issues.query(&self.login, window),
                        // A candidate can have earlier closes anywhere in the requested chunk.
                        bounds: SearchBounds::new(self.options.start),
                    },
                )
                .await?;
            let (
                Searches {
                    merge_involved,
                    merge_reviewed,
                    close_pulls,
                    close_issues,
                },
                refused,
            ) = node_refusals::read(answer, FIRST_SEARCH_LISTS, SKIPPABLE_REFUSALS)
                .map_err(GitHubApi::failure)?;
            scan.refused_candidates += refused;
            for (kind, first) in [
                (SearchKind::MergeInvolved, merge_involved),
                (SearchKind::MergeReviewed, merge_reviewed),
                (pulls, close_pulls),
                (issues, close_issues),
            ] {
                self.search_actions(kind, window, first, &mut allowance, &mut scan)
                    .await?;
            }
        }
        run.record_refused_candidates(scan.refused_candidates).await;
        scan.warn();
        Ok(())
    }

    async fn search_actions(
        &mut self,
        kind: SearchKind,
        window: CollectOptions,
        initial: SearchPage,
        allowance: &mut Allowance,
        scan: &mut StateChanges,
    ) -> Result<()> {
        let mut pending = vec![window];
        let mut initial = Some(initial);
        while let Some(window) = pending.pop() {
            let query = kind.query(&self.login, window);
            let first = if let Some(initial) = initial.take() {
                initial
            } else {
                if !spend(&mut allowance.search, pending.len()) {
                    scan.limited_searches += pending.len() + 1;
                    break;
                }
                let (page, refused) = self.search_action_page(kind, &query, None).await?;
                scan.refused_candidates += refused;
                page
            };
            let saturated = first.issue_count >= SEARCH_RESULTS;
            if saturated {
                if allowance.search.saturating_sub(pending.len()) >= 2
                    && let Some((left, right)) = split_window(window)
                {
                    pending.extend([right, left]);
                    continue;
                }
                scan.saturated_searches += 1;
            }

            let mut first = Some(first.page);
            let mut pager = Pager::new();
            for _ in 0..SEARCH_PAGES {
                if first.is_none()
                    && (!pager.has_next() || !spend(&mut allowance.search, pending.len()))
                {
                    break;
                }
                match kind {
                    SearchKind::MergeInvolved | SearchKind::MergeReviewed => {
                        let rows = self
                            .search_rows(
                                &mut pager,
                                &mut first,
                                kind,
                                &query,
                                scan,
                                |row: &MergeCandidate| &row.item.id,
                            )
                            .await?;
                        for row in rows {
                            self.merge_action(row, window, scan);
                        }
                    }
                    SearchKind::Close(expected) => {
                        let rows = self
                            .search_rows(
                                &mut pager,
                                &mut first,
                                kind,
                                &query,
                                scan,
                                |row: &CloseCandidate| &row.item.id,
                            )
                            .await?;
                        for row in rows {
                            self.close_actions(expected, row, allowance, scan).await?;
                        }
                    }
                }
            }
            if pager.has_next() && !saturated {
                scan.limited_searches += 1;
            }
        }
        Ok(())
    }

    async fn search_rows<T: DeserializeOwned>(
        &self,
        pager: &mut Pager,
        first: &mut Option<Page>,
        kind: SearchKind,
        query: &str,
        scan: &mut StateChanges,
        id: impl Fn(&T) -> &str,
    ) -> Result<Vec<T>> {
        let batch = pager
            .next::<_, anyhow::Error>(
                async |after| {
                    if let Some(first) = first.take() {
                        return Ok(first);
                    }
                    let (page, refused) = self.search_action_page(kind, query, after).await?;
                    scan.refused_candidates += refused;
                    Ok(page.page)
                },
                id,
            )
            .await?;
        match batch {
            Some(batch) if batch.skipped > 0 => {
                Err(CollectionError::StateChangeKindMismatch.into())
            }
            Some(batch) => Ok(batch.rows),
            None => Ok(Vec::new()),
        }
    }

    async fn search_action_page(
        &self,
        kind: SearchKind,
        query: &str,
        after: Option<&str>,
    ) -> Result<(SearchPage, usize)> {
        let merge = !matches!(kind, SearchKind::Close(_));
        let answer = self
            .api
            .graphql_answer(
                SEARCH,
                &SearchVariables {
                    query,
                    merge,
                    after,
                    bounds: SearchBounds::new(self.options.start),
                },
            )
            .await?;
        let alias = if merge { "merges" } else { "closes" };
        let (response, refused): (Search, _) =
            node_refusals::read(answer, &[&[alias, "nodes"]], SKIPPABLE_REFUSALS)
                .map_err(GitHubApi::failure)?;
        let page = if merge {
            response.merges
        } else {
            response.closes
        };
        Ok((
            page.ok_or_else(|| GitHubApi::failure(ApiError::Decode))?,
            refused,
        ))
    }

    fn merge_action(
        &mut self,
        candidate: MergeCandidate,
        window: CollectOptions,
        scan: &mut StateChanges,
    ) {
        let MergeCandidate {
            item,
            merged_by,
            merged_at,
            ..
        } = candidate;
        let (Some(Actor::User { id }), Some(timestamp)) = (merged_by, merged_at) else {
            return;
        };
        if id != self.viewer_id
            || !window.contains(timestamp.utc())
            || !scan.merged.insert(item.id.clone())
        {
            return;
        }
        let web_url = self.shown_url(item.url.as_deref());
        self.entries.push(entry(
            format!("{}:merged", item.id),
            timestamp,
            item.repository.name_with_owner.clone(),
            GitHubAction::Merged {
                pull_request: pull_request_ref(&item, web_url),
            },
        ));
    }

    async fn close_actions(
        &mut self,
        expected: CloseKind,
        candidate: CloseCandidate,
        allowance: &mut Allowance,
        scan: &mut StateChanges,
    ) -> Result<()> {
        let (kind, page) = candidate.timeline.into_parts();
        let item = candidate.item;
        if kind != expected {
            return Err(CollectionError::StateChangeKindMismatch.into());
        }
        if !scan.close_candidates.insert(item.id.clone()) {
            return Ok(());
        }
        let mut first = Some(page);
        let mut pager = Pager::new();
        for _ in 0..CLOSE_PAGES_PER_ITEM {
            if first.is_none() && (!pager.has_next() || !spend(&mut allowance.close, 0)) {
                break;
            }
            let batch = pager
                .next::<_, anyhow::Error>(
                    async |after| {
                        if let Some(first) = first.take() {
                            return Ok(first);
                        }
                        let after = after.ok_or(PageError::MissingNextCursor)?;
                        self.more_closes(kind, &item.id, after).await
                    },
                    |row: &CloseEvent| &row.id,
                )
                .await;
            let batch = match batch {
                Ok(Some(batch)) => batch,
                Ok(None) => break,
                Err(error)
                    if error.downcast_ref::<CollectionError>()
                        == Some(&CollectionError::CloseTimelineUnreadable) =>
                {
                    scan.unreadable_timelines += 1;
                    return Ok(());
                }
                Err(error) => return Err(error),
            };
            self.skipped += batch.skipped;
            for event in batch.rows {
                if matches!(&event.actor, Some(Actor::User { id }) if id == &self.viewer_id)
                    && self.options.contains(event.created_at.utc())
                    && scan.closes.insert(event.id.clone())
                {
                    let web_url = self.shown_url(item.url.as_deref());
                    let target = match kind {
                        CloseKind::PullRequest => {
                            Item::PullRequest(pull_request_ref(&item, web_url))
                        }
                        CloseKind::Issue => Item::Issue(IssueRef {
                            number: item.number.get(),
                            title: item.title.clone(),
                            web_url,
                        }),
                    };
                    self.entries.push(entry(
                        event.id,
                        event.created_at,
                        item.repository.name_with_owner.clone(),
                        GitHubAction::Closed { target },
                    ));
                }
            }
        }
        if pager.has_next() {
            scan.limited_timelines += 1;
        }
        Ok(())
    }

    async fn more_closes(&self, expected: CloseKind, id: &str, after: &str) -> Result<Page> {
        let answer = self
            .api
            .graphql_answer(
                MORE_CLOSE_EVENTS,
                &MoreCloseVariables {
                    id,
                    since: self.options.start,
                    after,
                    close_size: CLOSE_PAGE_SIZE,
                },
            )
            .await?;
        let nodes = RepositoryQuery::Node
            .read::<MoreCloseEvents>(answer)
            .map_err(GitHubApi::failure)?;
        let Some(RepositoryNode::Readable(node)) = nodes.into_iter().next() else {
            return Err(CollectionError::CloseTimelineUnreadable.into());
        };
        let (kind, page) = node.timeline.into_parts();
        if node.id != id || kind != expected {
            return Err(CollectionError::CloseTimelineMismatch.into());
        }
        Ok(page)
    }
}
