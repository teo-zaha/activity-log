use std::{
    collections::BTreeMap,
    fmt::Write as _,
    sync::{Mutex, PoisonError},
};

use futures::{FutureExt, future::BoxFuture};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use url::Origin;

use crate::{
    collect::{
        change_stack::{
            Budget, ChangeLookup, FailureKind, LookupCause, LookupFailure, OpenChange, OpenPage,
        },
        web_link::{Unshown, WebLink, web_link},
    },
    diagnostics,
    provider::{
        github::api::{
            GitHubApi,
            pager::{Page, PageError, Pager},
        },
        remote_api::{PostJsonError, RequestBudget, Retry},
        remote_error::{ApiError, OAuthRefusal, QueryAnswer, QueryErrorKind, QueryPathSegment},
    },
};

pub(super) const PULLS_PER_REQUEST: usize = 50;
pub(super) const CONCURRENCY: usize = 4;
const OPEN_PULLS: &str = concat!(
    include_str!("queries/open_pulls.graphql"),
    include_str!("queries/pull.graphql"),
);
const PULL_FRAGMENT: &str = include_str!("queries/pull.graphql");
const NATIVE_FRAGMENT: &str = concat!(
    include_str!("queries/native_entry.graphql"),
    include_str!("queries/pull.graphql"),
);
type Aliased<T> = Result<Option<T>, LookupFailure>;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub(super) struct GitHubRepo {
    owner: String,
    name: String,
}

impl GitHubRepo {
    pub(super) fn parse(value: &str) -> Option<Self> {
        let (owner, name) = value.split_once('/')?;
        if owner.trim().is_empty() || name.trim().is_empty() || name.contains('/') {
            return None;
        }
        Some(Self {
            owner: owner.to_ascii_lowercase(),
            name: name.to_ascii_lowercase(),
        })
    }

    fn check(&self, found: &str) -> Result<(), LookupFailure> {
        (Self::parse(found).as_ref() == Some(self))
            .then_some(())
            .ok_or_else(ignored_filter)
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(super) enum State {
    Open,
    Closed,
    Merged,
}

#[derive(Deserialize)]
pub(super) struct PullSummary {
    pub(super) number: u64,
    pub(super) title: String,
    pub(super) state: State,
    pub(super) url: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PullRow {
    #[serde(flatten)]
    pub(super) summary: PullSummary,
    id: String,
    head_ref_name: String,
    base_ref_name: String,
    is_cross_repository: bool,
}

#[derive(Deserialize)]
struct OpenPulls {
    #[serde(deserialize_with = "Option::deserialize")]
    repository: Option<PullList>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PullList {
    name_with_owner: String,
    pull_requests: Page,
}

#[derive(Clone, Copy)]
enum ListFilter<'a> {
    All,
    From(&'a str),
    Into(&'a str),
}

#[derive(Serialize)]
struct ListVariables<'a> {
    #[serde(flatten)]
    repo: &'a GitHubRepo,
    head: Option<&'a str>,
    base: Option<&'a str>,
    after: Option<&'a str>,
}

impl<'a> ListFilter<'a> {
    fn variables(self, repo: &'a GitHubRepo, after: Option<&'a str>) -> ListVariables<'a> {
        let (head, base) = match self {
            Self::All => (None, None),
            Self::From(branch) => (Some(branch), None),
            Self::Into(branch) => (None, Some(branch)),
        };
        ListVariables {
            repo,
            head,
            base,
            after,
        }
    }

    fn accepts(self, row: &PullRow) -> bool {
        row.summary.state == State::Open
            && match self {
                Self::All => true,
                Self::From(branch) => row.head_ref_name == branch,
                Self::Into(branch) => row.base_ref_name == branch,
            }
    }
}

#[derive(Clone, Copy)]
pub(super) enum AliasFields {
    Open,
    Native,
}

impl AliasFields {
    fn query(self, aliases: &[String]) -> String {
        let (operation, fields, fragments) = match self {
            Self::Open => ("OpenStackNumbers", "...StackPull", PULL_FRAGMENT),
            Self::Native => (
                "NativeStackNumbers",
                "...StackPull ...NativeStackEntry",
                NATIVE_FRAGMENT,
            ),
        };
        let mut query = format!("query {operation}($owner: String!, $name: String!");
        for alias in aliases {
            write!(query, ", ${alias}: Int!").expect("writing into a String");
        }
        query.push_str(") { repository(owner: $owner, name: $name) { nameWithOwner ");
        for alias in aliases {
            write!(
                query,
                "{alias}: pullRequest(number: ${alias}) {{ {fields} }} "
            )
            .expect("writing into a String");
        }
        query.push_str("} }\n");
        query.push_str(fragments);
        query
    }
}

#[derive(Default)]
struct Observations {
    skipped: usize,
    not_open: usize,
    elsewhere: Option<(usize, Origin)>,
}

pub(super) struct GitHubChanges<'a> {
    api: &'a GitHubApi,
    observations: Mutex<Observations>,
}

impl<'a> GitHubChanges<'a> {
    pub(super) fn new(api: &'a GitHubApi) -> Self {
        Self {
            api,
            observations: Mutex::default(),
        }
    }

    fn observe(&self, f: impl FnOnce(&mut Observations)) {
        f(&mut self
            .observations
            .lock()
            .unwrap_or_else(PoisonError::into_inner));
    }

    pub(super) fn log_observations(self) {
        let observed = self
            .observations
            .into_inner()
            .unwrap_or_else(PoisonError::into_inner);
        if observed.skipped > 0 {
            diagnostics::warn(
                "github",
                format!(
                    "Skipped {} unreadable pull-request rows during change-stack lookup",
                    observed.skipped
                ),
            );
        }
        if observed.not_open > 0 {
            diagnostics::info(
                "github",
                format!(
                    "Skipped {} pull requests that are no longer open",
                    observed.not_open
                ),
            );
        }
        if let Some((count, origin)) = observed.elsewhere {
            diagnostics::warn(
                "github",
                format!(
                    "Omitted {count} change-stack links to another origin, first: {}",
                    origin.ascii_serialization()
                ),
            );
        }
    }

    pub(super) fn not_open(&self) {
        self.observe(|observed| observed.not_open += 1);
    }

    fn link(&self, url: Option<&str>) -> Option<WebLink> {
        match web_link(url?, self.api.web_origin()) {
            Ok(link) => Some(link),
            Err(Unshown::Elsewhere(origin)) => {
                self.observe(|observed| observed.elsewhere.get_or_insert((0, origin)).0 += 1);
                None
            }
            Err(Unshown::NotAWebPage) => None,
        }
    }

    pub(super) fn change(&self, row: PullRow) -> Option<OpenChange> {
        let change = OpenChange::new(
            row.summary.number,
            row.summary.title,
            row.head_ref_name,
            row.base_ref_name,
            !row.is_cross_repository,
            self.link(row.summary.url.as_deref()),
        );
        if change.is_none() {
            self.observe(|observed| observed.skipped += 1);
        }
        change
    }

    async fn request<V: Serialize>(
        &self,
        query: &str,
        variables: &V,
        budget: &Budget,
    ) -> Result<QueryAnswer, LookupFailure> {
        budget.try_spend()?;
        self.api
            .graphql_answer_with(query, variables, Retry::Never)
            .await
            .map_err(|error| match error {
                PostJsonError::Remote(error) => request_failure(error),
                PostJsonError::Encode(error) => LookupFailure::caused_by(
                    FailureKind::Failed,
                    LookupCause::Encode {
                        kind: error.classify(),
                        detail: error.to_string(),
                    },
                ),
            })
    }

    pub(super) async fn aliases<T: DeserializeOwned>(
        &self,
        repo: &GitHubRepo,
        numbers: &[u64],
        fields: AliasFields,
        budget: &Budget,
    ) -> Result<Vec<Aliased<T>>, LookupFailure> {
        let mut variables = BTreeMap::from([
            ("owner".to_string(), json!(repo.owner)),
            ("name".to_string(), json!(repo.name)),
        ]);
        let mut aliases = Vec::with_capacity(numbers.len());
        for (index, &number) in numbers.iter().enumerate() {
            let number = i32::try_from(number)
                .ok()
                .filter(|&number| number > 0)
                .ok_or_else(ignored_filter)?;
            let alias = format!("n{index}");
            variables.insert(alias.clone(), json!(number));
            aliases.push(alias);
        }
        let answer = self
            .request(&fields.query(&aliases), &variables, budget)
            .await?;
        read_aliases(answer, repo, &aliases)
    }

    async fn read(
        &self,
        repo: &GitHubRepo,
        filter: ListFilter<'_>,
        budget: &Budget,
        first_only: bool,
    ) -> Result<(Vec<OpenChange>, bool), LookupFailure> {
        let mut pager = Pager::new();
        let mut changes = Vec::new();
        while let Some(batch) = pager
            .next::<_, LookupFailure>(
                async |after| {
                    let list: OpenPulls = self
                        .request(OPEN_PULLS, &filter.variables(repo, after), budget)
                        .await?
                        .complete()
                        .map_err(request_failure)?;
                    let list = list.repository.ok_or(FailureKind::Unreadable)?;
                    repo.check(&list.name_with_owner)?;
                    Ok(list.pull_requests)
                },
                |row: &PullRow| &row.id,
            )
            .await?
        {
            self.observe(|observed| observed.skipped += batch.skipped);
            for row in batch.rows {
                if !filter.accepts(&row) {
                    return Err(ignored_filter());
                }
                changes.extend(self.change(row));
            }
            if first_only {
                break;
            }
        }
        Ok((changes, pager.has_next()))
    }
}

impl ChangeLookup<GitHubRepo> for GitHubChanges<'_> {
    fn concurrency(&self) -> usize {
        CONCURRENCY
    }

    fn open_page<'b>(
        &'b self,
        repo: &'b GitHubRepo,
        budget: &'b Budget,
    ) -> BoxFuture<'b, Result<OpenPage, LookupFailure>> {
        async move {
            let (rows, more) = self.read(repo, ListFilter::All, budget, true).await?;
            Ok(if more {
                OpenPage::Partial(rows)
            } else {
                OpenPage::Complete(rows)
            })
        }
        .boxed()
    }

    fn open_numbers<'b>(
        &'b self,
        repo: &'b GitHubRepo,
        numbers: &'b [u64],
        budget: &'b Budget,
    ) -> BoxFuture<'b, Result<Vec<OpenChange>, LookupFailure>> {
        async move {
            let mut changes = Vec::new();
            for chunk in numbers.chunks(PULLS_PER_REQUEST) {
                let rows = self
                    .aliases::<PullRow>(repo, chunk, AliasFields::Open, budget)
                    .await?;
                for (&number, row) in chunk.iter().zip(rows) {
                    match row {
                        Ok(None) => {}
                        Ok(Some(row)) if row.summary.number == number => {
                            if row.summary.state == State::Open {
                                changes.extend(self.change(row));
                            } else {
                                self.not_open();
                            }
                        }
                        Ok(Some(_)) | Err(_) => self.observe(|observed| observed.skipped += 1),
                    }
                }
            }
            Ok(changes)
        }
        .boxed()
    }

    fn open_from<'b>(
        &'b self,
        repo: &'b GitHubRepo,
        branch: &'b str,
        budget: &'b Budget,
    ) -> BoxFuture<'b, Result<Vec<OpenChange>, LookupFailure>> {
        async move {
            if branch.trim().is_empty() {
                return Ok(Vec::new());
            }
            self.read(repo, ListFilter::From(branch), budget, false)
                .await
                .map(|(rows, _)| rows)
        }
        .boxed()
    }

    fn open_into<'b>(
        &'b self,
        repo: &'b GitHubRepo,
        branch: &'b str,
        budget: &'b Budget,
    ) -> BoxFuture<'b, Result<Vec<OpenChange>, LookupFailure>> {
        async move {
            if branch.trim().is_empty() {
                return Ok(Vec::new());
            }
            self.read(repo, ListFilter::Into(branch), budget, false)
                .await
                .map(|(rows, _)| rows)
        }
        .boxed()
    }
}

fn read_aliases<T: DeserializeOwned>(
    answer: QueryAnswer,
    repo: &GitHubRepo,
    aliases: &[String],
) -> Result<Vec<Aliased<T>>, LookupFailure> {
    use QueryPathSegment::Field;

    let (data, refusal) = answer.into_parts();
    let mut missing = vec![false; aliases.len()];
    if let Some(refusal) = refusal {
        for error in refusal.errors() {
            let alias = match error.path.as_deref() {
                Some([Field(root), Field(alias)])
                    if root == "repository" && error.kind == QueryErrorKind::NotFound =>
                {
                    Some(alias)
                }
                _ => None,
            };
            let Some(index) =
                alias.and_then(|alias| aliases.iter().position(|asked| asked == alias))
            else {
                return Err(request_failure(ApiError::Query(refusal)));
            };
            missing[index] = true;
        }
        if missing.iter().all(|&missing| missing) && data.as_ref().is_none_or(Value::is_null) {
            return Ok(missing.into_iter().map(|_| Ok(None)).collect());
        }
    }
    let Some(repository) = data.and_then(|mut data| data.as_object_mut()?.remove("repository"))
    else {
        return Err(request_failure(ApiError::Decode));
    };
    if repository.is_null() {
        return Err(FailureKind::Unreadable.into());
    }
    let Value::Object(mut repository) = repository else {
        return Err(request_failure(ApiError::Decode));
    };
    repo.check(
        repository
            .get("nameWithOwner")
            .and_then(Value::as_str)
            .ok_or_else(|| request_failure(ApiError::Decode))?,
    )?;
    Ok(aliases
        .iter()
        .zip(missing)
        .map(|(alias, missing)| {
            if missing {
                return Ok(None);
            }
            let row = repository
                .remove(alias)
                .ok_or_else(|| request_failure(ApiError::Decode))?;
            serde_json::from_value(row).map_err(|_| request_failure(ApiError::Decode))
        })
        .collect())
}

pub(super) fn ignored_filter() -> LookupFailure {
    LookupFailure::caused_by(FailureKind::Failed, LookupCause::IgnoredFilter)
}

pub(super) fn request_failure(error: ApiError) -> LookupFailure {
    let kind = if error.is_busy() {
        FailureKind::Throttled
    } else {
        match &error {
            ApiError::Status {
                refusal: Some(refusal),
                ..
            } => match refusal {
                OAuthRefusal::InvalidToken => FailureKind::TokenInvalid,
                OAuthRefusal::InsufficientScope => FailureKind::ScopeMissing,
                OAuthRefusal::InvalidRequest => FailureKind::Failed,
            },
            ApiError::Status {
                status: StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN,
                ..
            } => FailureKind::TokenInvalid,
            ApiError::Query(refusal)
                if refusal
                    .kinds()
                    .any(|kind| kind == QueryErrorKind::InsufficientScopes) =>
            {
                FailureKind::ScopeMissing
            }
            ApiError::Query(refusal)
                if refusal.kinds().all(|kind| {
                    matches!(kind, QueryErrorKind::Forbidden | QueryErrorKind::NotFound)
                }) =>
            {
                FailureKind::Unreadable
            }
            _ => FailureKind::Failed,
        }
    };
    LookupFailure::caused_by(kind, error)
}

impl From<PageError> for LookupFailure {
    fn from(error: PageError) -> Self {
        match error {
            PageError::RepeatedPage => {
                Self::caused_by(FailureKind::Failed, LookupCause::RepeatedPage)
            }
            PageError::MissingNextCursor => request_failure(ApiError::Decode),
        }
    }
}
