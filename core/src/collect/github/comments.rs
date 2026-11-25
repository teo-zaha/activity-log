use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::{Arc, Mutex},
};

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use tokio::sync::Mutex as AsyncMutex;

use crate::{
    diagnostics,
    model::{
        ConnectionId, Timestamp,
        github::{CommentTarget, GitHubAction, IssueRef},
    },
    provider::{
        github::api::{
            GitHubApi,
            pager::{Page, PageError, Pager},
        },
        remote_error::{ApiError, QueryAnswer, QueryError, QueryErrorKind, QueryPathSegment},
    },
};

use super::{
    Collection,
    commit_repositories::{RepositoryNode, RepositoryQuery},
    entry,
    error::CollectionError,
    pull_request_ref,
    response::{HostedItem, Viewer, nonblank},
};

pub(super) const ISSUE_COMMENTS: &str = include_str!("queries/issue_comments.graphql");
pub(super) const MORE_REVIEW_COMMENTS: &str = concat!(
    include_str!("queries/more_review_comments.graphql"),
    include_str!("queries/review_comment_page.graphql")
);

#[derive(Default)]
pub(crate) struct GitHubCollection {
    walks: Mutex<HashMap<ConnectionId, Arc<ConversationRun>>>,
}

impl GitHubCollection {
    pub(super) fn conversations(&self, connection: &ConnectionId) -> Arc<ConversationRun> {
        let mut walks = self.walks.lock().unwrap_or_else(|error| error.into_inner());
        walks
            .entry(connection.clone())
            .or_insert_with(|| Arc::new(ConversationRun::new(connection.to_string())))
            .clone()
    }
}

pub(super) struct ConversationRun {
    scope: String,
    state: AsyncMutex<ConversationState>,
}

impl Default for ConversationRun {
    fn default() -> Self {
        Self::new("github".into())
    }
}

impl ConversationRun {
    fn new(scope: String) -> Self {
        Self {
            scope,
            state: AsyncMutex::default(),
        }
    }
}

impl Drop for ConversationRun {
    fn drop(&mut self) {
        let count = self.state.get_mut().restricted;
        if count > 0 {
            diagnostics::warn(
                &self.scope,
                format!("Skipped {count} GitHub conversation comments the token could not read"),
            );
        }
    }
}

#[derive(Default)]
struct ConversationState {
    walk: ConversationWalk,
    restricted: usize,
}

struct ConversationWalk {
    pager: Pager,
    oldest_update: Option<DateTime<Utc>>,
    comments: BTreeMap<DateTime<Utc>, Vec<ConversationComment>>,
}

impl Default for ConversationWalk {
    fn default() -> Self {
        Self {
            pager: Pager::new(),
            oldest_update: None,
            comments: BTreeMap::new(),
        }
    }
}

#[derive(Serialize)]
struct ConversationVariables<'a> {
    after: Option<&'a str>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Conversations {
    issue_comments: Page,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConversationComment {
    #[serde(deserialize_with = "nonblank")]
    id: String,
    created_at: Timestamp,
    updated_at: Timestamp,
    #[serde(deserialize_with = "comment_body")]
    body: Option<String>,
    #[serde(deserialize_with = "Option::deserialize")]
    pull_request: Option<HostedItem>,
    issue: Option<HostedItem>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct InlineComment {
    #[serde(deserialize_with = "nonblank")]
    id: String,
    #[serde(deserialize_with = "nonblank")]
    path: String,
    #[serde(deserialize_with = "comment_body")]
    body: Option<String>,
}

#[derive(Serialize)]
struct MoreCommentVariables<'a> {
    id: &'a str,
    after: &'a str,
}

#[derive(Deserialize)]
struct ReviewComments {
    id: String,
    comments: Page,
}

impl Collection<'_> {
    pub(super) async fn conversation_comments(&mut self, run: &ConversationRun) -> Result<()> {
        let mut state = run.state.lock().await;
        // A failed or cancelled request can advance a pager. Publish the memo only on success.
        let mut walk = std::mem::take(&mut state.walk);
        while walk
            .oldest_update
            .is_none_or(|update| update >= self.options.start)
        {
            let Some(batch) = walk
                .pager
                .next::<_, anyhow::Error>(
                    async |after| {
                        let answer = self
                            .api
                            .graphql_answer(ISSUE_COMMENTS, &ConversationVariables { after })
                            .await?;
                        let (page, restricted) =
                            conversation_page(answer).map_err(GitHubApi::failure)?;
                        state.restricted += restricted;
                        Ok(page)
                    },
                    |row: &ConversationComment| &row.id,
                )
                .await?
            else {
                break;
            };
            self.skipped += batch.skipped;
            // Save the entire page, including rows that belong to a later, older chunk.
            for comment in batch.rows {
                let created_at = comment.created_at.utc();
                let updated_at = comment.updated_at.utc();
                if updated_at < created_at
                    || walk.oldest_update.is_some_and(|oldest| updated_at > oldest)
                {
                    return Err(CollectionError::CommentOrder.into());
                }
                walk.oldest_update = Some(updated_at);
                walk.comments.entry(created_at).or_default().push(comment);
            }
        }

        for (_, comments) in walk.comments.range(self.options.start..self.options.end) {
            for comment in comments {
                self.conversation_comment(comment);
            }
        }
        state.walk = walk;
        Ok(())
    }

    fn conversation_comment(&mut self, comment: &ConversationComment) {
        let (item, target) = if let Some(pull) = &comment.pull_request {
            let web_url = self.shown_url(pull.url.as_deref());
            (
                pull,
                CommentTarget::PullRequest(pull_request_ref(pull, web_url)),
            )
        } else if let Some(issue) = &comment.issue {
            let web_url = self.shown_url(issue.url.as_deref());
            (
                issue,
                CommentTarget::Issue(IssueRef {
                    number: issue.number.get(),
                    title: issue.title.clone(),
                    web_url,
                }),
            )
        } else {
            self.skipped += 1;
            return;
        };
        self.entries.push(entry(
            comment.id.clone(),
            comment.created_at.clone(),
            item.repository.name_with_owner.clone(),
            GitHubAction::CommentedOn {
                target,
                body: comment.body.clone(),
            },
        ));
    }

    pub(super) async fn inline_comments(
        &mut self,
        review_id: &str,
        submitted_at: &Timestamp,
        first: Page,
        item: &HostedItem,
    ) -> Result<()> {
        let mut first = Some(first);
        let mut pager = Pager::new();
        while let Some(batch) = pager
            .next_or_stop::<_, anyhow::Error>(
                async |after| {
                    if let Some(first) = first.take() {
                        return Ok(Some(first));
                    }
                    let after = after.ok_or(PageError::MissingNextCursor)?;
                    match more_comments(
                        self.api,
                        MoreCommentVariables {
                            id: review_id,
                            after,
                        },
                    )
                    .await?
                    {
                        RepositoryNode::Readable(page) => Ok(Some(page)),
                        RepositoryNode::Unavailable => {
                            self.unreadable_comment_repositories
                                .insert(item.repository.name_with_owner.clone());
                            Ok(None)
                        }
                    }
                },
                |row: &InlineComment| &row.id,
            )
            .await?
        {
            self.skipped += batch.skipped;
            for comment in batch.rows {
                let web_url = self.shown_url(item.url.as_deref());
                self.entries.push(entry(
                    comment.id,
                    submitted_at.clone(),
                    item.repository.name_with_owner.clone(),
                    GitHubAction::CommentedOn {
                        target: CommentTarget::ReviewLine {
                            pull_request: pull_request_ref(item, web_url),
                            path: comment.path,
                        },
                        body: comment.body,
                    },
                ));
            }
        }
        Ok(())
    }
}

fn conversation_page(answer: QueryAnswer) -> Result<(Page, usize), ApiError> {
    let (data, refusal) = answer.into_parts();
    let Some(refusal) = refusal else {
        let data = data.ok_or(ApiError::Decode)?;
        let answer: Viewer<Conversations> =
            serde_json::from_value(data).map_err(|_| ApiError::Decode)?;
        return Ok((answer.viewer.issue_comments, 0));
    };
    let Some(mut data) = data else {
        return Err(ApiError::Query(refusal));
    };
    let Some(nodes) = data
        .pointer_mut("/viewer/issueComments/nodes")
        .and_then(Value::as_array_mut)
    else {
        return Err(ApiError::Query(refusal));
    };
    let refused: Option<HashSet<_>> = refusal
        .errors()
        .iter()
        .map(|error| refused_comment_index(error, nodes.len()))
        .collect();
    let Some(refused) = refused else {
        return Err(ApiError::Query(refusal));
    };
    // Drop only the affected slots, even when a non-null comment object survived the refusal.
    let mut index = 0;
    nodes.retain(|_| {
        let keep = !refused.contains(&index);
        index += 1;
        keep
    });
    let answer: Viewer<Conversations> =
        serde_json::from_value(data).map_err(|_| ApiError::Query(refusal))?;
    Ok((answer.viewer.issue_comments, refused.len()))
}

fn refused_comment_index(error: &QueryError, rows: usize) -> Option<usize> {
    use QueryPathSegment::{Field, Index};
    match (error.kind, error.path.as_deref()) {
        (
            QueryErrorKind::Forbidden | QueryErrorKind::NotFound,
            Some(
                [
                    Field(viewer),
                    Field(comments),
                    Field(nodes),
                    Index(index),
                    ..,
                ],
            ),
        ) if viewer == "viewer"
            && comments == "issueComments"
            && nodes == "nodes"
            && *index < rows =>
        {
            Some(*index)
        }
        _ => None,
    }
}

async fn more_comments(
    api: &GitHubApi,
    variables: MoreCommentVariables<'_>,
) -> Result<RepositoryNode<Page>> {
    let answer = api.graphql_answer(MORE_REVIEW_COMMENTS, &variables).await?;
    let nodes = RepositoryQuery::Node
        .read::<ReviewComments>(answer)
        .map_err(GitHubApi::failure)?;
    match nodes
        .into_iter()
        .next()
        .ok_or_else(|| GitHubApi::failure(ApiError::Decode))?
    {
        RepositoryNode::Unavailable => Ok(RepositoryNode::Unavailable),
        RepositoryNode::Readable(response) => {
            if response.id != variables.id {
                return Err(CollectionError::CommentReviewMismatch.into());
            }
            Ok(RepositoryNode::Readable(response.comments))
        }
    }
}

fn comment_body<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    let body = String::deserialize(deserializer)?;
    let body = body.trim();
    Ok((!body.is_empty()).then(|| body.to_owned()))
}
