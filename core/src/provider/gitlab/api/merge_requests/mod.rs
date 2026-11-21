//! A project's open merge requests, as GitLab lists them (`GET /projects/:id/merge_requests`).

use reqwest::Url;
use serde::Deserialize;

use super::{GitLabApi, PagedRow};

/// Rows a page of merge requests holds; a page is heavy (authors, labels…), but one page is
/// fewer round trips than several small ones.
const PAGE_SIZE: usize = 100;

/// What a merge request list asks for, besides being open. Never no filter in disguise: it
/// holds no empty number list and no blank branch, which GitLab would read as no filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MergeRequestFilter<'a>(Filter<'a>);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Filter<'a> {
    All,
    Numbers(&'a [u64]),
    From(&'a str),
    Into(&'a str),
}

/// How a row fits the question that listed it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RowFit {
    /// It is what was asked for.
    Asked,
    /// It was asked for but is no longer open, as one being merged (`locked`) can be listed
    /// for a moment: it is left out.
    NotOpen,
    /// It is of another project, number or branch: the server ignored the filter, so nothing
    /// it sent can be trusted.
    NotAsked,
}

impl<'a> MergeRequestFilter<'a> {
    /// Every open merge request.
    pub(crate) const ALL: Self = Self(Filter::All);

    /// Those with these numbers (`iids[]`); none for no numbers.
    pub(crate) fn numbers(numbers: &'a [u64]) -> Option<Self> {
        (!numbers.is_empty()).then_some(Self(Filter::Numbers(numbers)))
    }

    /// Those from this source branch; none for a blank branch.
    pub(crate) fn from_branch(branch: &'a str) -> Option<Self> {
        (!branch.trim().is_empty()).then_some(Self(Filter::From(branch)))
    }

    /// Those into this target branch; none for a blank branch.
    pub(crate) fn into_branch(branch: &'a str) -> Option<Self> {
        (!branch.trim().is_empty()).then_some(Self(Filter::Into(branch)))
    }

    /// How `row` of `project`'s list fits this question.
    pub(crate) fn fit(&self, project: u64, row: &MergeRequestRow) -> RowFit {
        let asked = row.target_project_id == project
            && match self.0 {
                Filter::All => true,
                Filter::Numbers(numbers) => numbers.contains(&row.iid),
                Filter::From(branch) => row.source_branch == branch,
                Filter::Into(branch) => row.target_branch == branch,
            };
        match (asked, row.state) {
            (false, _) => RowFit::NotAsked,
            (true, MergeRequestState::Opened) => RowFit::Asked,
            (true, MergeRequestState::Other) => RowFit::NotOpen,
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
            match filter.0 {
                Filter::All => {}
                Filter::Numbers(numbers) => {
                    for number in numbers {
                        query.append_pair("iids[]", &number.to_string());
                    }
                }
                Filter::From(branch) => {
                    query.append_pair("source_branch", branch);
                }
                Filter::Into(branch) => {
                    query.append_pair("target_branch", branch);
                }
            }
        }
        url
    }
}
