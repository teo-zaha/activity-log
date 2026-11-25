use std::collections::HashSet;

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    model::{CollectOptions, Timestamp, github::GitHubAction},
    provider::{
        github::api::{
            GitHubApi,
            pager::{Page, PageError, Pager},
        },
        remote_error::ApiError,
    },
};

use super::{
    Collection, CollectionError, Viewer,
    commit_repositories::{RepositoryNode, RepositoryQuery},
    entry,
    response::{Repository, nonblank},
    split_window,
};

pub(super) const REPOSITORIES: &str = include_str!("queries/commit_repositories.graphql");
pub(super) const HISTORY: &str = include_str!("queries/commit_history.graphql");
pub(super) const MORE_HISTORY: &str = include_str!("queries/more_commit_history.graphql");
const REPOSITORIES_PER_REQUEST: usize = 20;
const DISCOVERY_REPOSITORY_LIMIT: usize = 100;

#[derive(Serialize)]
struct Bounds {
    from: DateTime<Utc>,
    to: DateTime<Utc>,
}

impl From<CollectOptions> for Bounds {
    fn from(options: CollectOptions) -> Self {
        Self {
            from: options.start,
            to: options.end,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DiscoveryVariables {
    max_repositories: usize,
    #[serde(flatten)]
    bounds: Bounds,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HistoryVariables<'a> {
    ids: &'a [String],
    viewer_id: &'a str,
    #[serde(flatten)]
    bounds: Bounds,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MoreHistoryVariables<'a> {
    id: &'a str,
    viewer_id: &'a str,
    after: &'a str,
    #[serde(flatten)]
    bounds: Bounds,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommitContributions {
    contributions_collection: Discovery,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Discovery {
    restricted_contributions_count: u64,
    total_repositories_with_contributed_commits: usize,
    commit_contributions_by_repository: Vec<ContributionRepository>,
}

#[derive(Deserialize)]
struct ContributionRepository {
    repository: RepositoryId,
}

#[derive(Deserialize)]
struct RepositoryId {
    #[serde(deserialize_with = "nonblank")]
    id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoryRepository {
    #[serde(deserialize_with = "nonblank")]
    id: String,
    #[serde(flatten)]
    repository: Repository,
    #[serde(deserialize_with = "Option::deserialize")]
    default_branch_ref: Option<Branch>,
}

#[derive(Deserialize)]
struct Branch {
    #[serde(deserialize_with = "nonblank")]
    name: String,
    target: Target,
}

#[derive(Deserialize)]
struct Target {
    history: Page,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Commit {
    #[serde(deserialize_with = "nonblank")]
    id: String,
    #[serde(deserialize_with = "commit_oid")]
    oid: String,
    committed_date: Timestamp,
    message_headline: String,
    url: Option<String>,
}

fn commit_oid<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    use serde::de::Error as _;

    let value = String::deserialize(deserializer)?;
    if !matches!(value.len(), 40 | 64) || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(D::Error::custom("expected a full Git object ID"));
    }
    Ok(value.to_ascii_lowercase())
}

impl Collection<'_> {
    pub(super) async fn commits_in(&mut self, window: CollectOptions) -> Result<()> {
        let repositories = self.commit_repositories(window).await?;
        for ids in repositories.chunks(REPOSITORIES_PER_REQUEST) {
            let answer = self
                .api
                .graphql_answer(
                    HISTORY,
                    &HistoryVariables {
                        ids,
                        viewer_id: &self.viewer_id,
                        bounds: window.into(),
                    },
                )
                .await?;
            let nodes = RepositoryQuery::Nodes(ids.len())
                .read::<HistoryRepository>(answer)
                .map_err(GitHubApi::failure)?;
            for (id, node) in ids.iter().zip(nodes) {
                if let RepositoryNode::Readable(repository) = node {
                    if repository.id != *id {
                        return Err(CollectionError::CommitRepositoryMismatch.into());
                    }
                    match self.repository_commits(repository, window).await {
                        Ok(()) => continue,
                        Err(error)
                            if error.downcast_ref::<CollectionError>()
                                == Some(&CollectionError::CommitRepositoryUnavailable) => {}
                        Err(error) => return Err(error),
                    }
                }
                self.unreadable_repositories.insert(id.clone());
            }
        }
        Ok(())
    }

    async fn commit_repositories(&mut self, window: CollectOptions) -> Result<Vec<String>> {
        let mut pending = vec![window];
        let mut repositories = HashSet::new();
        while let Some(window) = pending.pop() {
            let response: Viewer<CommitContributions> = self
                .api
                .graphql(
                    REPOSITORIES,
                    &DiscoveryVariables {
                        max_repositories: DISCOVERY_REPOSITORY_LIMIT,
                        bounds: window.into(),
                    },
                )
                .await?;
            let discovery = response.viewer.contributions_collection;
            self.restricted = self
                .restricted
                .max(discovery.restricted_contributions_count);
            let at_limit =
                discovery.commit_contributions_by_repository.len() >= DISCOVERY_REPOSITORY_LIMIT;
            let found: HashSet<_> = discovery
                .commit_contributions_by_repository
                .into_iter()
                .map(|row| row.repository.id)
                .collect();
            let missing = discovery
                .total_repositories_with_contributed_commits
                .saturating_sub(found.len());
            if at_limit && missing > 0 {
                let (left, right) =
                    split_window(window).ok_or(CollectionError::IncompleteCommitDiscovery)?;
                pending.extend([right, left]);
            } else {
                self.undiscovered_repositories = self.undiscovered_repositories.max(missing);
                repositories.extend(found);
            }
        }
        let mut repositories = repositories.into_iter().collect::<Vec<_>>();
        repositories.sort_unstable();
        Ok(repositories)
    }

    async fn repository_commits(
        &mut self,
        repository: HistoryRepository,
        window: CollectOptions,
    ) -> Result<()> {
        let Some(branch) = repository.default_branch_ref else {
            return Ok(());
        };
        let branch_name = branch.name;
        let mut first = Some(branch.target.history);
        let mut pager = Pager::new();
        let mut commits = Vec::new();
        while let Some(batch) = pager
            .next::<_, anyhow::Error>(
                async |after| {
                    if let Some(first) = first.take() {
                        return Ok(first);
                    }
                    let after = after.ok_or(PageError::MissingNextCursor)?;
                    let branch = next_history(
                        self.api,
                        MoreHistoryVariables {
                            id: &repository.id,
                            viewer_id: &self.viewer_id,
                            after,
                            bounds: window.into(),
                        },
                    )
                    .await?;
                    if branch.name != branch_name {
                        return Err(CollectionError::CommitBranchChanged.into());
                    }
                    Ok(branch.target.history)
                },
                |row: &Commit| &row.id,
            )
            .await?
        {
            self.skipped += batch.skipped;
            commits.extend(
                batch
                    .rows
                    .into_iter()
                    .filter(|commit| window.contains(commit.committed_date.utc())),
            );
        }
        for commit in commits {
            let web_url = self.shown_url(commit.url.as_deref());
            self.entries.push(entry(
                commit.id,
                commit.committed_date,
                repository.repository.name_with_owner.clone(),
                GitHubAction::Committed {
                    sha: commit.oid,
                    message: (!commit.message_headline.trim().is_empty())
                        .then_some(commit.message_headline),
                    branch: branch_name.clone(),
                    web_url,
                },
            ));
        }
        Ok(())
    }
}

async fn next_history(api: &GitHubApi, variables: MoreHistoryVariables<'_>) -> Result<Branch> {
    let answer = api.graphql_answer(MORE_HISTORY, &variables).await?;
    let nodes = RepositoryQuery::Node
        .read::<HistoryRepository>(answer)
        .map_err(GitHubApi::failure)?;
    let [node]: [RepositoryNode<HistoryRepository>; 1] = nodes
        .try_into()
        .map_err(|_| GitHubApi::failure(ApiError::Decode))?;
    let RepositoryNode::Readable(response) = node else {
        return Err(CollectionError::CommitRepositoryUnavailable.into());
    };
    if response.id != variables.id {
        return Err(CollectionError::CommitRepositoryMismatch.into());
    }
    response
        .default_branch_ref
        .ok_or_else(|| CollectionError::CommitBranchChanged.into())
}
