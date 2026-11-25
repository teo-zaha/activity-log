mod commit_repositories;
mod commits;
mod error;
mod pager;
mod response;

use std::collections::HashSet;

use anyhow::{Result, anyhow};
use chrono::{Days, SecondsFormat};
use serde::Serialize;
use url::{Host, Origin};

use crate::{
    collect::web_link::{Unshown, web_link},
    diagnostics,
    model::{
        CollectOptions, ConnectionName, GitHubTag, Timestamp,
        github::{GitHubAction, GitHubEntry, GitHubReviewState, IssueRef, Item, PullRequestRef},
    },
    provider::{
        github::{GitHubConfig, api::GitHubApi},
        provider_context::ProviderContext,
    },
};

use error::CollectionError;
use pager::{Page, Pager};
use response::{
    Contributions, HostedItem, MoreReviews, Opened, OwnPullRequest, OwnPullRequests, Review,
    ReviewCandidate, ReviewContribution, ReviewState, User, Viewer, ViewerUser,
};

const VIEWER: &str = include_str!("queries/viewer.graphql");
const OPENED_PULL_REQUESTS: &str = include_str!("queries/opened_pull_requests.graphql");
const OPENED_ISSUES: &str = include_str!("queries/opened_issues.graphql");
const REVIEWED_PULL_REQUESTS: &str = include_str!("queries/reviewed_pull_requests.graphql");
const OWN_PULL_REQUESTS: &str = include_str!("queries/own_pull_requests.graphql");
const MORE_REVIEWS: &str = include_str!("queries/more_reviews.graphql");
const CONTRIBUTION_WINDOW_DAYS: Days = Days::new(364);

pub(crate) async fn collect_github(
    options: &CollectOptions,
    context: &ProviderContext,
) -> Result<Vec<GitHubEntry>> {
    let config = context.config::<GitHubConfig>()?;
    let api = GitHubApi::new(config).map_err(|error| anyhow!(error.reason(GitHubApi::SERVICE)))?;
    collect(&api, *options).await
}

async fn collect(api: &GitHubApi, options: CollectOptions) -> Result<Vec<GitHubEntry>> {
    let Viewer {
        viewer: ViewerUser {
            id,
            user: User { login },
        },
    } = api.graphql(VIEWER, &ViewerVariables {}).await?;
    let mut collection = Collection {
        api,
        options,
        login,
        viewer_id: id,
        entries: Vec::new(),
        reviewed_pulls: HashSet::new(),
        skipped: 0,
        restricted: 0,
        links_elsewhere: None,
        unreadable_repositories: HashSet::new(),
        undiscovered_repositories: 0,
    };
    for window in windows(options) {
        collection.opened(window, OpenKind::PullRequest).await?;
        collection.opened(window, OpenKind::Issue).await?;
        collection.review_contributions(window).await?;
        collection.commits_in(window).await?;
    }
    collection.own_pull_requests().await?;

    if collection.restricted > 0 {
        diagnostics::warn(
            "github",
            format!(
                "{} GitHub contributions are in repositories this token cannot read",
                collection.restricted
            ),
        );
    }
    if !collection.unreadable_repositories.is_empty() {
        diagnostics::warn(
            "github",
            format!(
                "Could not read commit history for {} GitHub repositories. Check repository access and Contents permission.",
                collection.unreadable_repositories.len()
            ),
        );
    }
    if collection.undiscovered_repositories > 0 {
        diagnostics::warn(
            "github",
            format!(
                "GitHub listed {} fewer repositories than it reported in a commit contribution window. Check repository access.",
                collection.undiscovered_repositories
            ),
        );
    }
    if collection.skipped > 0 {
        diagnostics::warn(
            "github",
            format!(
                "Skipped {} GitHub rows that could not be read",
                collection.skipped
            ),
        );
    }
    if let Some((count, origin)) = collection.links_elsewhere {
        diagnostics::warn(
            "github",
            format!(
                "Omitted {count} GitHub links to another origin, first: {}",
                origin.ascii_serialization()
            ),
        );
    }

    Ok(collection.entries)
}

/// The API limits each contribution range, even when the runtime collects without a cache.
fn windows(options: CollectOptions) -> impl Iterator<Item = CollectOptions> {
    let end = options.end;
    std::iter::successors(Some(options.start), move |start| {
        start
            .checked_add_days(CONTRIBUTION_WINDOW_DAYS)
            .filter(|next| *next < end)
    })
    .take_while(move |start| *start < end)
    .map(move |start| CollectOptions {
        start,
        end: start
            .checked_add_days(CONTRIBUTION_WINDOW_DAYS)
            .map_or(end, |next| next.min(end)),
    })
}

#[derive(Serialize)]
struct ViewerVariables {}

#[derive(Serialize)]
struct WindowVariables<'a> {
    from: String,
    to: String,
    after: Option<&'a str>,
}

impl<'a> WindowVariables<'a> {
    fn new(window: CollectOptions, after: Option<&'a str>) -> Self {
        Self {
            from: window.start.to_rfc3339_opts(SecondsFormat::AutoSi, true),
            to: window.end.to_rfc3339_opts(SecondsFormat::AutoSi, true),
            after,
        }
    }
}

#[derive(Serialize)]
struct ReviewContributionVariables<'a> {
    #[serde(flatten)]
    window: WindowVariables<'a>,
    login: &'a str,
}

#[derive(Serialize)]
struct OwnPullRequestVariables<'a> {
    login: &'a str,
    after: Option<&'a str>,
}

#[derive(Serialize)]
struct MoreReviewVariables<'a> {
    id: &'a str,
    login: &'a str,
    after: &'a str,
}

enum OpenKind {
    PullRequest,
    Issue,
}

struct Collection<'a> {
    api: &'a GitHubApi,
    options: CollectOptions,
    login: String,
    viewer_id: String,
    entries: Vec<GitHubEntry>,
    reviewed_pulls: HashSet<String>,
    skipped: usize,
    restricted: u64,
    links_elsewhere: Option<(usize, Origin)>,
    unreadable_repositories: HashSet<String>,
    undiscovered_repositories: usize,
}

impl Collection<'_> {
    async fn contribution_page(
        &mut self,
        query: &'static str,
        variables: &impl Serialize,
    ) -> Result<Page> {
        let response: Viewer<Contributions> = self.api.graphql(query, variables).await?;
        let page = response.viewer.contributions_collection;
        self.restricted = self.restricted.max(page.restricted_contributions_count);
        Ok(page.page)
    }

    async fn opened(&mut self, window: CollectOptions, kind: OpenKind) -> Result<()> {
        let query = match kind {
            OpenKind::PullRequest => OPENED_PULL_REQUESTS,
            OpenKind::Issue => OPENED_ISSUES,
        };
        let mut pager = Pager::new();
        while let Some(batch) = pager
            .next(
                async |after| {
                    self.contribution_page(query, &WindowVariables::new(window, after))
                        .await
                },
                |row: &Opened| &row.item.id,
            )
            .await?
        {
            self.skipped += batch.skipped;
            for Opened { occurred_at, item } in batch.rows {
                if !window.contains(occurred_at.utc()) {
                    continue;
                }
                let web_url = self.shown_url(item.url.as_deref());
                let target = match kind {
                    OpenKind::PullRequest => Item::PullRequest(pull_request_ref(&item, web_url)),
                    OpenKind::Issue => Item::Issue(IssueRef {
                        number: item.number.get(),
                        title: item.title,
                        web_url,
                    }),
                };
                self.entries.push(entry(
                    item.id,
                    occurred_at,
                    item.repository.name_with_owner,
                    GitHubAction::Opened { target },
                ));
            }
        }
        Ok(())
    }

    async fn review_contributions(&mut self, window: CollectOptions) -> Result<()> {
        let login = self.login.clone();
        let mut pager = Pager::new();
        while let Some(batch) = pager
            .next(
                async |after| {
                    self.contribution_page(
                        REVIEWED_PULL_REQUESTS,
                        &ReviewContributionVariables {
                            window: WindowVariables::new(window, after),
                            login: &login,
                        },
                    )
                    .await
                },
                |row: &ReviewContribution| &row.pull_request.item.id,
            )
            .await?
        {
            self.skipped += batch.skipped;
            for row in batch.rows {
                self.reviews(row.pull_request).await?;
            }
        }
        Ok(())
    }

    /// A reply on a PR the viewer opened need not appear in review contributions.
    async fn own_pull_requests(&mut self) -> Result<()> {
        let mut pager = Pager::new();
        while let Some(batch) = pager
            .next(
                async |after| {
                    let response: Viewer<OwnPullRequests> = self
                        .api
                        .graphql(
                            OWN_PULL_REQUESTS,
                            &OwnPullRequestVariables {
                                login: &self.login,
                                after,
                            },
                        )
                        .await?;
                    Ok(response.viewer.page)
                },
                |row: &OwnPullRequest| &row.candidate.item.id,
            )
            .await?
        {
            self.skipped += batch.skipped;
            for row in batch.rows {
                if row.updated_at.utc() < self.options.start {
                    return Ok(());
                }
                self.reviews(row.candidate).await?;
            }
        }
        Ok(())
    }

    async fn reviews(&mut self, candidate: ReviewCandidate) -> Result<()> {
        let ReviewCandidate { item, reviews } = candidate;
        if !self.reviewed_pulls.insert(item.id.clone()) {
            return Ok(());
        }
        // The contribution points to a PR. Read every review in the requested range, since
        // contribution windows may name the same PR.
        let mut first = Some(reviews);
        let mut pager = Pager::new();
        while let Some(batch) = pager
            .next(
                async |after| {
                    if let Some(first) = first.take() {
                        return Ok(first);
                    }
                    let after = after.ok_or(CollectionError::MissingNextCursor)?;
                    let response: MoreReviews = self
                        .api
                        .graphql(
                            MORE_REVIEWS,
                            &MoreReviewVariables {
                                id: &item.id,
                                login: &self.login,
                                after,
                            },
                        )
                        .await?;
                    if response.node.id != item.id {
                        return Err(CollectionError::ReviewPullRequestMismatch.into());
                    }
                    Ok(response.node.reviews)
                },
                |row: &Review| &row.id,
            )
            .await?
        {
            self.skipped += batch.skipped;
            for review in batch.rows {
                self.review(review, &item)?;
            }
        }
        Ok(())
    }

    fn review(&mut self, review: Review, item: &HostedItem) -> Result<()> {
        let state = match review.state {
            ReviewState::Approved => GitHubReviewState::Approved,
            ReviewState::ChangesRequested => GitHubReviewState::ChangesRequested,
            ReviewState::Commented => GitHubReviewState::Commented,
            ReviewState::Dismissed => GitHubReviewState::Dismissed,
            ReviewState::Pending => return Ok(()),
        };
        let (Some(timestamp), Some(author)) = (review.submitted_at, review.author) else {
            self.skipped += 1;
            return Ok(());
        };
        if !author.login.eq_ignore_ascii_case(&self.login) {
            return Err(CollectionError::ReviewAuthorMismatch.into());
        }
        if self.options.contains(timestamp.utc()) {
            let web_url = self.shown_url(item.url.as_deref());
            self.entries.push(entry(
                review.id,
                timestamp,
                item.repository.name_with_owner.clone(),
                GitHubAction::Reviewed {
                    pull_request: pull_request_ref(item, web_url),
                    state,
                },
            ));
        }
        Ok(())
    }

    fn shown_url(&mut self, raw: Option<&str>) -> Option<String> {
        match github_web_link(raw?, self.api.web_origin()) {
            Ok(link) => Some(link),
            Err(Unshown::Elsewhere(origin)) => {
                self.links_elsewhere.get_or_insert((0, origin)).0 += 1;
                None
            }
            Err(Unshown::NotAWebPage) => None,
        }
    }
}

fn github_web_link(raw: &str, allowed: &Origin) -> Result<String, Unshown> {
    match web_link(raw, allowed) {
        Err(Unshown::Elsewhere(origin)) if same_dns_origin(&origin, allowed) => {
            web_link(raw, &origin)
        }
        result => result,
    }
    .map(|link| link.as_str().to_owned())
}

/// GitHub accepts a fully qualified DNS name for its web host. Treat the final dot
/// as equivalent for link display, while keeping the configured origin for secrets.
fn same_dns_origin(left: &Origin, right: &Origin) -> bool {
    match (left, right) {
        (
            Origin::Tuple(left_scheme, Host::Domain(left_host), left_port),
            Origin::Tuple(right_scheme, Host::Domain(right_host), right_port),
        ) => {
            left_scheme == right_scheme
                && left_port == right_port
                && left_host.strip_suffix('.').unwrap_or(left_host)
                    == right_host.strip_suffix('.').unwrap_or(right_host)
        }
        _ => false,
    }
}

fn pull_request_ref(item: &HostedItem, web_url: Option<String>) -> PullRequestRef {
    PullRequestRef {
        number: item.number.get(),
        title: item.title.clone(),
        web_url,
        change_stack: None,
    }
}

fn entry(
    id: String,
    timestamp: Timestamp,
    repository: String,
    action: GitHubAction,
) -> GitHubEntry {
    GitHubEntry {
        source: GitHubTag,
        connection: ConnectionName::unstamped(),
        id,
        timestamp,
        repository,
        action,
    }
}

#[cfg(test)]
mod tests;
