use std::{
    collections::{HashMap, HashSet},
    fmt::Write as _,
};

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::{
    diagnostics,
    model::{
        CollectOptions, Timestamp,
        github::{GitHubAction, ReleaseRef},
    },
    provider::github::api::{
        GitHubApi,
        pager::{Page, PageError, Pager},
    },
};

use super::{
    Collection, CollectionError,
    commit_repositories::{
        REPOSITORIES_PER_REQUEST, RepositoryBatch, RepositoryNode, RepositoryQuery,
    },
    entry,
    response::{Repository, nonblank},
};

pub(super) const RELEASES_PER_PAGE: usize = 100;
pub(super) const RELEASE_PAGES_PER_REPOSITORY: usize = 3;
pub(super) const RELEASE_CONTINUATION_REQUESTS_PER_WINDOW: usize = 24;
pub(super) const RELEASE_NODES: &[&str] = &["releases", "nodes"];
pub(super) const MORE_RELEASES_OPERATION: &str = "query MoreReleases(";

#[derive(Deserialize)]
pub(super) struct ReleaseRepository {
    #[serde(deserialize_with = "nonblank")]
    id: String,
    #[serde(flatten)]
    repository: Repository,
    releases: Page,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Release {
    #[serde(deserialize_with = "nonblank")]
    id: String,
    #[serde(deserialize_with = "nonblank")]
    tag_name: String,
    name: Option<String>,
    url: Option<String>,
    created_at: Timestamp,
    published_at: Option<Timestamp>,
    is_draft: bool,
    is_prerelease: bool,
    author: Option<ReleaseAuthor>,
}

#[derive(Deserialize)]
struct ReleaseAuthor {
    id: String,
}

struct ReleasePage {
    oldest: Option<DateTime<Utc>>,
    rows: Vec<Release>,
}

struct ReleaseHistory {
    repository: String,
    pager: Pager,
    pages: Vec<ReleasePage>,
}

impl ReleaseHistory {
    fn new(repository: String) -> Self {
        Self {
            repository,
            pager: Pager::new(),
            pages: Vec::new(),
        }
    }

    async fn read_page(&mut self, page: Page, refused: usize) -> Result<(usize, bool), PageError> {
        let batch = self
            .pager
            .next::<_, PageError>(async |_| Ok(page), |row: &Release| &row.id)
            .await?
            .ok_or(PageError::MissingNextCursor)?;
        let unreadable = batch.rows.is_empty() && (batch.skipped > 0 || refused > 0);
        self.pages.push(ReleasePage {
            oldest: batch.rows.iter().map(|row| row.created_at.utc()).min(),
            rows: batch.rows,
        });
        Ok((batch.skipped, unreadable))
    }

    fn last_page_for(&self, start: DateTime<Utc>) -> Option<usize> {
        self.pages
            .iter()
            .position(|page| page.oldest.is_none_or(|date| date < start))
    }

    fn rows_for(&self, start: DateTime<Utc>) -> impl Iterator<Item = &Release> {
        let pages = self
            .last_page_for(start)
            .map_or(self.pages.as_slice(), |end| &self.pages[..=end]);
        pages.iter().flat_map(|page| &page.rows)
    }
}

/// Release pages are reusable by every cache chunk of a connection's collection.
#[derive(Default)]
pub(super) struct ReleaseScan {
    repositories: HashMap<String, ReleaseHistory>,
    unreadable: HashSet<String>,
    cut_off: HashSet<String>,
    refused_rows: usize,
}

impl ReleaseScan {
    pub(super) fn needs_first_page(&self, id: &str) -> bool {
        !self.repositories.contains_key(id) && !self.unreadable.contains(id)
    }

    pub(super) async fn read_pages(
        &mut self,
        ids: &[&str],
        batch: RepositoryBatch<ReleaseRepository>,
    ) -> Result<usize> {
        let mut skipped = 0;
        for ((&id, node), refused) in ids.iter().zip(batch.nodes).zip(batch.refused_by_repository) {
            self.refused_rows += refused;
            let RepositoryNode::Readable(repository) = node else {
                self.unreadable.insert(id.into());
                continue;
            };
            if repository.id != id {
                return Err(CollectionError::ReleaseRepositoryMismatch.into());
            }
            let history = self
                .repositories
                .entry(id.into())
                .or_insert_with(|| ReleaseHistory::new(repository.repository.name_with_owner));
            let (unreadable_rows, unreadable_page) =
                history.read_page(repository.releases, refused).await?;
            skipped += unreadable_rows;
            if unreadable_page {
                self.cut_off.insert(id.into());
            }
        }
        Ok(skipped)
    }

    fn continuations<'a>(
        &mut self,
        repositories: &'a [String],
        start: DateTime<Utc>,
        requests_left: usize,
    ) -> Result<Vec<ReleaseRequest<'a>>, PageError> {
        let mut batch = Vec::new();
        for id in repositories {
            if self.unreadable.contains(id) {
                continue;
            }
            let Some(history) = self.repositories.get(id) else {
                continue;
            };
            if history.last_page_for(start).is_some() || !history.pager.has_next() {
                continue;
            }
            if history.pages.len() >= RELEASE_PAGES_PER_REPOSITORY || requests_left == 0 {
                self.cut_off.insert(id.clone());
            } else if batch.len() < REPOSITORIES_PER_REQUEST
                && let Some(cursor) = history.pager.next_cursor()?
            {
                batch.push(ReleaseRequest {
                    id,
                    cursor: cursor.to_owned(),
                });
            }
        }
        Ok(batch)
    }

    pub(super) fn report(&self, scope: &str) {
        if !self.cut_off.is_empty() {
            diagnostics::warn(
                scope,
                format!(
                    "Stopped reading GitHub releases early in {} repositories",
                    self.cut_off.len()
                ),
            );
        }
        let unreadable = self.unreadable.len();
        if unreadable > 0 {
            diagnostics::warn(
                scope,
                format!(
                    "Could not read all releases for {unreadable} GitHub repositories. Check repository access and Contents permission.",
                ),
            );
        }
        if self.refused_rows > 0 {
            diagnostics::warn(
                scope,
                format!(
                    "Skipped {} GitHub releases the token could not read",
                    self.refused_rows
                ),
            );
        }
    }
}

struct ReleaseRequest<'a> {
    id: &'a str,
    cursor: String,
}

impl Collection<'_> {
    pub(super) async fn releases_in(
        &mut self,
        options: CollectOptions,
        ids: &[String],
        scan: &mut ReleaseScan,
    ) -> Result<()> {
        for remaining in (0..=RELEASE_CONTINUATION_REQUESTS_PER_WINDOW).rev() {
            let requests = scan.continuations(ids, options.start, remaining)?;
            if requests.is_empty() {
                break;
            }
            let pages = next_releases(self.api, &requests).await?;
            let requested: Vec<_> = requests.iter().map(|request| request.id).collect();
            self.skipped += scan.read_pages(&requested, pages).await?;
        }
        for id in ids {
            let Some(history) = scan.repositories.get(id) else {
                continue;
            };
            for row in history.rows_for(options.start) {
                if row.is_draft
                    || row
                        .author
                        .as_ref()
                        .is_none_or(|author| author.id != self.viewer_id)
                {
                    continue;
                }
                let Some(timestamp) = row
                    .published_at
                    .as_ref()
                    .filter(|at| options.contains(at.utc()))
                else {
                    continue;
                };
                let web_url = self.shown_url(row.url.as_deref());
                self.entries.push(entry(
                    row.id.clone(),
                    timestamp.clone(),
                    history.repository.clone(),
                    GitHubAction::Released {
                        release: ReleaseRef {
                            tag: row.tag_name.clone(),
                            name: row
                                .name
                                .as_ref()
                                .filter(|name| !name.trim().is_empty())
                                .cloned(),
                            web_url,
                            prerelease: row.is_prerelease,
                        },
                    },
                ));
            }
        }
        Ok(())
    }
}

async fn next_releases(
    api: &GitHubApi,
    requests: &[ReleaseRequest<'_>],
) -> Result<RepositoryBatch<ReleaseRepository>> {
    // Alias and variable names come from response positions, never from repository IDs or cursors.
    let mut declaration = format!("{MORE_RELEASES_OPERATION}$pageSize: Int!");
    let mut fields = String::new();
    let mut variables = Map::<String, Value>::new();
    variables.insert("pageSize".into(), json!(RELEASES_PER_PAGE));
    let mut aliases = Vec::new();
    for (index, request) in requests.iter().enumerate() {
        let alias = format!("r{index}");
        write!(declaration, ", ${alias}Id: ID!, ${alias}After: String!")
            .expect("writing to a string");
        writeln!(
            fields,
            "{alias}: node(id: ${alias}Id) {{ ... on Repository {{ id nameWithOwner \
             releases(first: $pageSize, after: ${alias}After, orderBy: {{ field: CREATED_AT, direction: DESC }}) \
             {{ ...ReleasePage }} }} }}"
        ).expect("writing to a string");
        variables.insert(format!("{alias}Id"), json!(request.id));
        variables.insert(format!("{alias}After"), json!(request.cursor));
        aliases.push(alias);
    }
    let fragment = include_str!("queries/release_page.graphql");
    let query = format!("{declaration}) {{\n{fields}}}\n{fragment}");
    let answer = api.graphql_answer(&query, &variables).await?;
    RepositoryQuery::Aliases(&aliases)
        .read_with_row_skips(answer, RELEASE_NODES)
        .map_err(|error| GitHubApi::failure(error).into())
}
