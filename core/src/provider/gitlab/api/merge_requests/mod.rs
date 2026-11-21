//! A project's open merge requests, as GitLab lists them (`GET /projects/:id/merge_requests`).

use reqwest::Url;
use serde::Deserialize;

use super::{GitLabApi, PagedRow};

/// Rows a page of merge requests holds; a page is heavy (authors, labels…), but one page is
/// fewer round trips than several small ones.
const PAGE_SIZE: usize = 100;

/// What a merge request list asks for, besides being open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MergeRequestFilter<'a> {
    /// Every open merge request.
    All,
    /// Those with these numbers (`iids[]`); never empty, which would read as no filter.
    Numbers(&'a [u64]),
    /// Those from this source branch; never blank, which would read as no filter.
    From(&'a str),
    /// Those into this target branch; never blank.
    Into(&'a str),
}

impl MergeRequestFilter<'_> {
    /// Whether `row` of `project`'s list is one this filter asks for, and open: a row that is
    /// not was sent by a server that ignored part of the request.
    pub(crate) fn asks_for(&self, project: u64, row: &MergeRequestRow) -> bool {
        row.state == MergeRequestState::Opened
            && row.target_project_id == project
            && match self {
                Self::All => true,
                Self::Numbers(numbers) => numbers.contains(&row.iid),
                Self::From(branch) => row.source_branch == *branch,
                Self::Into(branch) => row.target_branch == *branch,
            }
    }
}

/// A merge request's state as GitLab names it; only an open one is asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum MergeRequestState {
    Opened,
    /// Closed, merged, locked, or a state this version does not know.
    #[serde(other)]
    Other,
}

/// One merge request of a list, with the fields a change stack takes from it.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct MergeRequestRow {
    /// Unique on the server, unlike `iid`: what the pager knows a row by.
    pub id: u64,
    /// Its number in its target project.
    pub iid: u64,
    pub title: String,
    pub state: MergeRequestState,
    pub source_branch: String,
    pub target_branch: String,
    /// The project its source branch is in: another for a fork, `null` for a deleted fork.
    pub source_project_id: Option<u64>,
    pub target_project_id: u64,
    pub web_url: Option<String>,
}

impl PagedRow for MergeRequestRow {
    fn row_id(&self) -> u64 {
        self.id
    }
}

impl GitLabApi {
    /// The first page of `project`'s open merge requests that `filter` asks for, of every
    /// author (`scope=all`, also on a server whose default is another).
    pub(crate) fn open_merge_requests(&self, project: u64, filter: MergeRequestFilter) -> Url {
        let mut url = self.endpoint(&["projects", &project.to_string(), "merge_requests"]);
        {
            let mut query = url.query_pairs_mut();
            query
                .append_pair("state", "opened")
                .append_pair("scope", "all")
                .append_pair("per_page", &PAGE_SIZE.to_string())
                .append_pair("page", "1");
            match filter {
                MergeRequestFilter::All => {}
                MergeRequestFilter::Numbers(numbers) => {
                    for number in numbers {
                        query.append_pair("iids[]", &number.to_string());
                    }
                }
                MergeRequestFilter::From(branch) => {
                    query.append_pair("source_branch", branch);
                }
                MergeRequestFilter::Into(branch) => {
                    query.append_pair("target_branch", branch);
                }
            }
        }
        url
    }
}
