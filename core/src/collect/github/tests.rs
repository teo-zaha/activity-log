use serde_json::{Value, json};

use super::comments::{ISSUE_COMMENTS, MORE_REVIEW_COMMENTS};
use super::*;
use crate::model::github::CommentTarget;
use crate::{
    model::{ConnectionId, IdentityEvidence, LogLevel, ProviderProblem, RemoteIdentity, Source},
    provider::{
        remote_api::{
            identity_check,
            test_server::{Reply, Request, TestServer},
        },
        remote_error::{ApiError, QueryErrorKind, RemoteFailure},
    },
};

const TOKEN: &str = "github-example-token";
const START: &str = "2026-11-01T00:00:00Z";
const END: &str = "2026-11-21T00:00:00Z";
const IN_RANGE: &str = "2026-11-12T00:00:00Z";

fn api(server: &TestServer) -> GitHubApi {
    GitHubApi::new(&GitHubConfig {
        url: server.url.clone(),
        token: TOKEN.into(),
    })
    .unwrap()
}

async fn collect(api: &GitHubApi, options: CollectOptions) -> Result<Vec<GitHubEntry>> {
    super::collect(api, options, &Default::default()).await
}

fn options() -> CollectOptions {
    CollectOptions {
        start: START.parse().unwrap(),
        end: END.parse().unwrap(),
    }
}

fn page(nodes: Vec<Value>, after: Option<&str>) -> Value {
    json!({ "pageInfo": { "hasNextPage": after.is_some(), "endCursor": after }, "nodes": nodes })
}

fn contributions(nodes: Vec<Value>, after: Option<&str>) -> Value {
    json!({ "viewer": { "contributionsCollection": {
        "restrictedContributionsCount": 0, "page": page(nodes, after),
    } } })
}

fn review(id: &str, state: &str, timestamp: Option<&str>) -> Value {
    json!({
        "id": id, "state": state, "submittedAt": timestamp,
        "author": { "login": "jdoe" }, "comments": page(vec![], None),
    })
}

fn item(id: &str, number: u64) -> Value {
    json!({ "id": id, "number": number, "title": "Build widgets", "repository": { "nameWithOwner": "acme/widgets" } })
}

fn with_url(mut row: Value, url: String) -> Value {
    row["url"] = json!(url);
    row
}

fn pull(id: &str, number: u64, updated_at: &str, reviews: Value) -> Value {
    let mut item = item(id, number);
    item["updatedAt"] = json!(updated_at);
    item["reviews"] = reviews;
    item
}

fn request_body(request: &Request) -> Value {
    assert_eq!(request.method, "POST");
    assert_eq!(request.path(), "/api/graphql");
    assert_eq!(
        request.header("authorization"),
        Some("Bearer github-example-token")
    );
    serde_json::from_slice(&request.body).unwrap()
}

fn answer(data: Value) -> Reply {
    Reply::json(200, json!({ "data": data }).to_string())
}

#[tokio::test]
async fn collects_openings_and_each_submitted_review_across_distinct_pages() {
    let server = TestServer::start(|request| {
        let body = request_body(request);
        let variables = &body["variables"];
        let origin = format!("http://{}", request.header("host").unwrap());
        if !variables["from"].is_null() {
            assert_eq!(variables["from"], START);
            assert_eq!(variables["to"], END);
        }
        if !variables["login"].is_null() {
            assert_eq!(variables["login"], "jdoe");
        }

        match body["query"].as_str().unwrap() {
            VIEWER => answer(json!({ "viewer": { "id": "U_example", "login": "jdoe" } })),
            OPENED_PULL_REQUESTS if variables["after"].is_null() => answer(contributions(
                vec![
                    json!({ "occurredAt": START,
                        "item": with_url(item("pr-a", 12), format!("{origin}/acme/widgets/pull/12"))
                    }),
                    json!(null),
                ],
                Some("open-page-2"),
            )),
            OPENED_PULL_REQUESTS => {
                assert_eq!(variables["after"], "open-page-2");
                answer(contributions(
                    vec![json!({ "occurredAt": IN_RANGE, "item": with_url(
                        item("pr-b", 13),
                        "https://other.example.org/acme/widgets/pull/13".into(),
                    ) })],
                    None,
                ))
            }
            OPENED_ISSUES => answer(contributions(
                vec![
                    json!({ "occurredAt": IN_RANGE,
                        "item": with_url(item("issue-a", 7), format!("{origin}/acme/widgets/issues/7"))
                    }),
                    json!({ "occurredAt": END, "item": item("issue-b", 8) }),
                ],
                None,
            )),
            REVIEWED_PULL_REQUESTS => answer(contributions(
                vec![
                    json!({ "pullRequest": with_url(pull("pr-a", 12, IN_RANGE, page(
                    vec![review("review-a", "APPROVED", Some(IN_RANGE)), review("pending", "PENDING", None)],
                    Some("review-page-2"),
                )), format!("{origin}/acme/widgets/pull/12")) }),
                ],
                None,
            )),
            MORE_REVIEWS => {
                assert_eq!(variables["id"], "pr-a");
                assert_eq!(variables["after"], "review-page-2");
                answer(json!({ "node": { "id": "pr-a", "reviews": page(
                    vec![
                        review("review-a", "APPROVED", Some(IN_RANGE)),
                        review("review-b", "CHANGES_REQUESTED", Some(IN_RANGE)),
                        review("late", "DISMISSED", Some(END)),
                    ], None,
                ) } }))
            }
            ISSUE_COMMENTS => conversations(vec![], None),
            commits::REPOSITORIES => answer(commit_discovery(&[], 0)),
            OWN_PULL_REQUESTS => {
                assert!(variables["after"].is_null());
                answer(json!({ "viewer": { "page": page(vec![
                    pull("pr-a", 12, IN_RANGE, page(vec![], None)),
                    with_url(pull("pr-b", 13, IN_RANGE, page(
                        vec![review("self-review", "COMMENTED", Some(IN_RANGE))],
                        None,
                    )), "https://elsewhere.example.org/acme/widgets/pull/13".into()),
                    pull("pr-old", 1, "2025-02-01T00:00:00Z", page(vec![], None)),
                ], Some("older-pulls")) } }))
            }
            other => panic!("unexpected operation {other}"),
        }
    });
    let connection = ConnectionId::new(Source::Github, "review-paging-links");
    let scope = connection.to_string();
    let api = api(&server);
    let entries = diagnostics::in_connection(connection, collect(&api, options()))
        .await
        .unwrap();
    let warnings: Vec<_> = diagnostics::entries()
        .into_iter()
        .filter(|row| {
            row.scope == scope && row.level == LogLevel::Warn && row.message.starts_with("Omitted")
        })
        .map(|row| row.message)
        .collect();
    assert_eq!(
        warnings,
        ["Omitted 2 GitHub links to another origin, first: https://other.example.org"]
    );
    assert_eq!(server.requests().len(), 9);
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>(),
        [
            "pr-a",
            "pr-b",
            "issue-a",
            "review-a",
            "review-b",
            "self-review"
        ]
    );
    let pull_url = format!("{}/acme/widgets/pull/12", server.url.trim_end_matches('/'));
    let issue_url = format!("{}/acme/widgets/issues/7", server.url.trim_end_matches('/'));
    assert!(matches!(
        &entries[0].action,
        GitHubAction::Opened { target: Item::PullRequest(pull) }
        if pull.web_url.as_deref() == Some(pull_url.as_str())
    ));
    assert!(matches!(
        &entries[1].action,
        GitHubAction::Opened { target: Item::PullRequest(pull) } if pull.web_url.is_none()
    ));
    assert!(matches!(
        &entries[2].action,
        GitHubAction::Opened { target: Item::Issue(issue) }
        if issue.number == 7 && issue.web_url.as_deref() == Some(issue_url.as_str())
    ));
    assert!(matches!(
        &entries[4].action,
        GitHubAction::Reviewed { pull_request, state: GitHubReviewState::ChangesRequested }
        if pull_request.number == 12 && pull_request.web_url.as_deref() == Some(pull_url.as_str())
    ));
    assert!(matches!(
        &entries[5].action,
        GitHubAction::Reviewed { pull_request, state: GitHubReviewState::Commented }
        if pull_request.number == 13 && pull_request.web_url.is_none()
    ));
    assert_eq!(entries[0].timestamp.as_str(), START);
    assert!(
        entries
            .iter()
            .all(|entry| entry.repository == "acme/widgets")
    );
}

#[test]
fn a_trailing_dot_is_equivalent_only_for_the_matching_web_origin() {
    use reqwest::Url;

    let origin = Url::parse("https://github.com.").unwrap().origin();
    let raw = "https://github.com/acme/widgets/pull/12";
    assert_eq!(github_web_link(raw, &origin).unwrap().as_str(), raw);

    for different in [
        "http://github.com/acme/widgets/pull/12",
        "https://github.com:8443/acme/widgets/pull/12",
        "https://github.com.example.org/acme/widgets/pull/12",
    ] {
        let elsewhere = Url::parse(different).unwrap().origin();
        assert_eq!(
            github_web_link(different, &origin),
            Err(Unshown::Elsewhere(elsewhere))
        );
    }
}

#[tokio::test]
async fn a_partial_contribution_answer_fails_collection_with_its_typed_cause() {
    let server = TestServer::start(|request| {
        let body = request_body(request);
        if body["query"] == VIEWER {
            return answer(json!({ "viewer": { "id": "U_example", "login": "jdoe" } }));
        }
        Reply::json(
            200,
            json!({
                "data": contributions(vec![], None),
                "errors": [{ "type": "RATE_LIMITED", "message": "Example limit" }]
            })
            .to_string(),
        )
    });
    let error = collect(&api(&server), options()).await.unwrap_err();
    let remote = error.downcast::<RemoteFailure>().unwrap();
    assert_eq!(remote.error.problem(), Some(ProviderProblem::Throttled));
    assert!(
        matches!(remote.error, ApiError::Query(refusal) if refusal.kinds().eq([QueryErrorKind::RateLimited]))
    );
    assert_eq!(server.requests().len(), 2);
}

#[tokio::test]
async fn a_repeated_cursor_stops_before_it_is_requested_again() {
    let server = TestServer::start(|request| {
        let body = request_body(request);
        if body["query"] == VIEWER {
            return answer(json!({ "viewer": { "id": "U_example", "login": "jdoe" } }));
        }
        assert_eq!(body["query"], OPENED_PULL_REQUESTS);
        answer(contributions(vec![], Some("same-cursor")))
    });
    let error = collect(&api(&server), options()).await.unwrap_err();
    assert_eq!(
        error.downcast::<PageError>().unwrap(),
        PageError::RepeatedPage
    );
    assert_eq!(server.requests().len(), 3);
}

#[tokio::test]
async fn startup_identity_is_the_bearer_tokens_owner() {
    let server = TestServer::start(|request| {
        assert_eq!(request.method, "GET");
        assert_eq!(request.path(), "/api/v3/user");
        assert_eq!(
            request.header("authorization"),
            Some("Bearer github-example-token")
        );
        assert_eq!(request.header("x-github-api-version"), Some("2022-11-28"));
        assert_eq!(
            request.header("accept"),
            Some("application/vnd.github+json")
        );
        Reply::json(200, r#"{"id":42,"login":"jdoe"}"#)
    });
    let checked = identity_check(GitHubApi::SERVICE, api(&server).current_user().await);
    assert_eq!(
        checked.identity,
        IdentityEvidence::Verified(RemoteIdentity {
            id: "42".into(),
            username: "jdoe".into(),
        })
    );
}

#[test]
fn contribution_windows_cover_the_requested_range_without_overlap() {
    let start = START.parse::<chrono::DateTime<chrono::Utc>>().unwrap();
    let end = start + Days::new(900);
    assert_eq!(
        windows(CollectOptions { start, end }).collect::<Vec<_>>(),
        [
            CollectOptions {
                start,
                end: start + Days::new(364)
            },
            CollectOptions {
                start: start + Days::new(364),
                end: start + Days::new(728)
            },
            CollectOptions {
                start: start + Days::new(728),
                end
            },
        ],
    );
}

#[tokio::test]
async fn wrong_review_pages_keep_their_protocol_error_kind() {
    for expected in [
        CollectionError::ReviewPullRequestMismatch,
        CollectionError::ReviewAuthorMismatch,
    ] {
        let server = TestServer::start(move |request| {
            let body = request_body(request);
            match body["query"].as_str().unwrap() {
                VIEWER => answer(json!({ "viewer": { "id": "U_example", "login": "jdoe" } })),
                OPENED_PULL_REQUESTS | OPENED_ISSUES => answer(contributions(vec![], None)),
                REVIEWED_PULL_REQUESTS => {
                    let reviews = match expected {
                        CollectionError::ReviewPullRequestMismatch => page(vec![], Some("second")),
                        CollectionError::ReviewAuthorMismatch => {
                            let mut row = review("review-a", "APPROVED", Some(IN_RANGE));
                            row["author"]["login"] = json!("other-example-user");
                            page(vec![row], None)
                        }
                        _ => unreachable!("checked separately"),
                    };
                    answer(contributions(
                        vec![json!({ "pullRequest": pull("pr-a", 12, IN_RANGE, reviews) })],
                        None,
                    ))
                }
                MORE_REVIEWS => answer(json!({
                    "node": { "id": "pr-other", "reviews": page(vec![], None) }
                })),
                other => panic!("unexpected operation {other}"),
            }
        });
        let error = collect(&api(&server), options()).await.unwrap_err();
        assert_eq!(error.downcast::<CollectionError>().unwrap(), expected);
    }
}

// Live read-only check: use a repository with one viewer commit whose authored and committed
// dates fall in different UTC windows. In each window compare CommitRepositories' total to
// the returned list length, then use CommitHistory to check dates and exact range endpoints.
// Record the typed error paths from refused history requests, including MoreCommitHistory
// when there is a continuation. Record shapes and counts only.

fn commit_discovery(ids: &[&str], total: usize) -> Value {
    let repositories = ids
        .iter()
        .map(|id| json!({"repository": {"id": id}}))
        .collect::<Vec<_>>();
    json!({"viewer": {"contributionsCollection": {
        "restrictedContributionsCount": 0,
        "totalRepositoriesWithContributedCommits": total,
        "commitContributionsByRepository": repositories,
    }}})
}

fn full_commit_discovery() -> Value {
    let ids: Vec<_> = (0..100).map(|i| format!("repo-{i}")).collect();
    let names: Vec<_> = ids.iter().map(String::as_str).collect();
    commit_discovery(&names, 101)
}

fn history_repository(id: &str, history: Value) -> Value {
    json!({
        "id": id,
        "nameWithOwner": "acme/widgets",
        "defaultBranchRef": {"name": "main", "target": {"history": history}},
    })
}

const COMMIT_SHA: &str = "0123456789abcdef0123456789abcdef01234567";

fn commit_row(id: &str, date: &str) -> Value {
    json!({
        "id": id,
        "oid": COMMIT_SHA,
        "committedDate": date,
        "messageHeadline": "Build widgets",
    })
}

fn standard_query(query: &str) -> Option<Reply> {
    match query {
        VIEWER => Some(answer(
            json!({"viewer": {"id": "U_example", "login": "jdoe"}}),
        )),
        OPENED_PULL_REQUESTS | OPENED_ISSUES | REVIEWED_PULL_REQUESTS => {
            Some(answer(contributions(vec![], None)))
        }
        ISSUE_COMMENTS => Some(conversations(vec![], None)),
        OWN_PULL_REQUESTS => Some(answer(json!({"viewer": {"page": page(vec![], None)}}))),
        _ => None,
    }
}

#[tokio::test]
async fn commit_history_splits_discovery_and_keeps_only_completed_readable_repositories() {
    const MID: &str = "2026-11-11T00:00:00Z";
    let server = TestServer::start(|request| {
        let body = request_body(request);
        let query = body["query"].as_str().unwrap();
        let variables = &body["variables"];
        if let Some(reply) = standard_query(query) {
            return reply;
        }
        match query {
            commits::REPOSITORIES => {
                assert_eq!(variables["maxRepositories"], 100);
                let bounds = (
                    variables["from"].as_str().unwrap(),
                    variables["to"].as_str().unwrap(),
                );
                match bounds {
                    (START, END) => answer(full_commit_discovery()),
                    (START, MID) => answer(commit_discovery(&["repo-a"], 1)),
                    (MID, END) => answer(commit_discovery(
                        &["repo-a", "repo-b", "repo-c", "repo-d", "repo-e"],
                        5,
                    )),
                    _ => panic!("unexpected bounds {bounds:?}"),
                }
            }
            commits::HISTORY => {
                assert_eq!(
                    variables["ids"],
                    json!(["repo-a", "repo-b", "repo-c", "repo-d", "repo-e"])
                );
                assert_eq!(variables["viewerId"], "U_example");
                assert_eq!(variables["from"], START);
                assert_eq!(variables["to"], END);
                let mut first_commit = commit_row("C_start", START);
                let host = request.header("host").unwrap();
                first_commit["url"] =
                    json!(format!("http://{host}/acme/widgets/commit/{COMMIT_SHA}"));
                let data = json!({"nodes": [
                    history_repository("repo-a", page(vec![first_commit, commit_row("C_end", END)], Some("a-next"))),
                    {},
                    null,
                    {"id": "repo-d", "nameWithOwner": "acme/empty", "defaultBranchRef": null},
                    history_repository("repo-e", page(vec![commit_row("C_partial", IN_RANGE)], Some("e-next"))),
                ]});
                Reply::json(
                    200,
                    json!({"data": data, "errors": [{
                        "type": "FORBIDDEN", "path": ["nodes", 1, "defaultBranchRef"]
                    }]})
                    .to_string(),
                )
            }
            commits::MORE_HISTORY => match variables["id"].as_str().unwrap() {
                "repo-a" => {
                    assert_eq!(variables["after"], "a-next");
                    let mut row = commit_row("C_in", IN_RANGE);
                    row["messageHeadline"] = json!(" ");
                    answer(json!({"node": history_repository("repo-a", page(vec![row], None))}))
                }
                "repo-e" => {
                    assert_eq!(variables["after"], "e-next");
                    Reply::json(
                        200,
                        json!({"data": {"node": null}, "errors": [{
                            "type": "NOT_FOUND", "path": ["node", "defaultBranchRef", "target", "history"]
                        }]})
                        .to_string(),
                    )
                }
                id => panic!("unexpected continuation {id}"),
            },
            _ => panic!("unexpected operation {query}"),
        }
    });
    let connection = ConnectionId::new(Source::Github, "commit-history");
    let scope = connection.to_string();
    let entries = diagnostics::in_connection(connection, collect(&api(&server), options()))
        .await
        .unwrap();
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>(),
        ["C_start", "C_in"]
    );
    assert!(matches!(
        &entries[0].action,
        GitHubAction::Committed { sha, branch, message, web_url }
        if sha == COMMIT_SHA && branch == "main" && message.as_deref() == Some("Build widgets")
            && web_url.as_deref() == Some(format!("{}/acme/widgets/commit/{COMMIT_SHA}", server.url.trim_end_matches('/')).as_str())
    ));
    assert!(matches!(
        entries[1].action,
        GitHubAction::Committed { message: None, .. }
    ));
    assert_eq!(
        diagnostics::entries()
            .into_iter()
            .filter(|row| row.scope == scope && row.level == LogLevel::Warn)
            .map(|row| row.message)
            .collect::<Vec<_>>(),
        [
            "Could not read commit history for 3 GitHub repositories. Check repository access and Contents permission."
        ]
    );
}

#[tokio::test]
async fn a_truncated_one_day_discovery_fails_the_window() {
    let server = TestServer::start(|request| {
        let body = request_body(request);
        let query = body["query"].as_str().unwrap();
        if let Some(reply) = standard_query(query) {
            return reply;
        }
        assert_eq!(query, commits::REPOSITORIES);
        answer(full_commit_discovery())
    });
    let start = options().start;
    let error = collect(
        &api(&server),
        CollectOptions {
            start,
            end: start + Days::new(1),
        },
    )
    .await
    .unwrap_err();
    assert_eq!(
        error.downcast_ref::<CollectionError>(),
        Some(&CollectionError::IncompleteCommitDiscovery)
    );
}

#[tokio::test]
async fn discovery_below_the_cap_warns_and_keeps_available_commits() {
    let server = TestServer::start(|request| {
        let body = request_body(request);
        let query = body["query"].as_str().unwrap();
        if let Some(reply) = standard_query(query) {
            return reply;
        }
        match query {
            commits::REPOSITORIES => {
                assert_eq!(body["variables"]["from"], START);
                assert_eq!(body["variables"]["to"], END);
                answer(commit_discovery(&["repo-a"], 3))
            }
            commits::HISTORY => answer(json!({"nodes": [
                history_repository("repo-a", page(vec![commit_row("C_start", START)], None))
            ]})),
            _ => panic!("unexpected operation {query}"),
        }
    });
    let connection = ConnectionId::new(Source::Github, "short-discovery");
    let scope = connection.to_string();
    let entries = diagnostics::in_connection(connection, collect(&api(&server), options()))
        .await
        .unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, "C_start");
    assert_eq!(
        diagnostics::entries()
            .into_iter()
            .filter(|row| row.scope == scope && row.level == LogLevel::Warn)
            .map(|row| row.message)
            .collect::<Vec<_>>(),
        [
            "GitHub listed 2 fewer repositories than it reported in a commit contribution window. Check repository access."
        ],
    );
}

#[tokio::test]
async fn a_repeated_commit_history_page_fails_the_window() {
    let server = TestServer::start(|request| {
        let body = request_body(request);
        let query = body["query"].as_str().unwrap();
        if let Some(reply) = standard_query(query) {
            return reply;
        }
        match query {
            commits::REPOSITORIES => answer(commit_discovery(&["repo-a"], 1)),
            commits::HISTORY => answer(json!({"nodes": [
                history_repository("repo-a", page(vec![commit_row("C_start", START)], Some("second")))
            ]})),
            commits::MORE_HISTORY => answer(json!({"node":
                history_repository("repo-a", page(vec![commit_row("C_start", START)], None))
            })),
            _ => panic!("unexpected operation {query}"),
        }
    });
    let error = collect(&api(&server), options()).await.unwrap_err();
    assert_eq!(
        error.downcast_ref::<PageError>(),
        Some(&PageError::RepeatedPage)
    );
}

#[tokio::test]
async fn unrelated_and_non_access_history_errors_fail_the_window() {
    for (code, path, kind) in [
        ("FORBIDDEN", json!(["viewer"]), QueryErrorKind::Forbidden),
        ("FORBIDDEN", json!(["nodes", 1]), QueryErrorKind::Forbidden),
        (
            "RATE_LIMITED",
            json!(["nodes", 0]),
            QueryErrorKind::RateLimited,
        ),
    ] {
        let server = TestServer::start(move |request| {
            let body = request_body(request);
            let query = body["query"].as_str().unwrap();
            if let Some(reply) = standard_query(query) {
                return reply;
            }
            match query {
                commits::REPOSITORIES => answer(commit_discovery(&["repo-a"], 1)),
                commits::HISTORY => Reply::json(
                    200,
                    json!({"data": {"nodes": [null]}, "errors": [{"type": code, "path": path}]})
                        .to_string(),
                ),
                _ => panic!("unexpected operation {query}"),
            }
        });
        let error = collect(&api(&server), options()).await.unwrap_err();
        let remote = error.downcast::<RemoteFailure>().unwrap();
        assert!(matches!(remote.error, ApiError::Query(refusal) if refusal.kinds().eq([kind])));
    }
}

#[tokio::test]
async fn history_rejects_a_changed_repository_or_default_branch() {
    use CollectionError::{CommitBranchChanged, CommitRepositoryMismatch};
    for (first_id, more_id, branch, expected) in [
        ("repo-b", "repo-a", "main", CommitRepositoryMismatch),
        ("repo-a", "repo-b", "main", CommitRepositoryMismatch),
        ("repo-a", "repo-a", "next", CommitBranchChanged),
    ] {
        let server = TestServer::start(move |request| {
            let body = request_body(request);
            let query = body["query"].as_str().unwrap();
            if let Some(reply) = standard_query(query) {
                return reply;
            }
            match query {
                commits::REPOSITORIES => answer(commit_discovery(&["repo-a"], 1)),
                commits::HISTORY => answer(json!({"nodes": [
                    history_repository(first_id, page(vec![commit_row("C_start", START)], Some("second")))
                ]})),
                commits::MORE_HISTORY => {
                    let mut row =
                        history_repository(more_id, page(vec![commit_row("C_in", IN_RANGE)], None));
                    row["defaultBranchRef"]["name"] = json!(branch);
                    answer(json!({"node": row}))
                }
                _ => panic!("unexpected operation {query}"),
            }
        });
        let error = collect(&api(&server), options()).await.unwrap_err();
        assert_eq!(error.downcast_ref::<CollectionError>(), Some(&expected));
    }
}

fn conversations(rows: Vec<Value>, after: Option<&str>) -> Reply {
    answer(json!({ "viewer": { "issueComments": page(rows, after) } }))
}

fn conversation(id: &str, created: &str, updated: &str, is_pull: bool, body: &str) -> Value {
    json!({
        "id": id, "createdAt": created, "updatedAt": updated, "body": body,
        "issue": item("issue-example", 7),
        "pullRequest": is_pull.then(|| item("pull-example", 12)),
    })
}

// Live check: verify an edited conversation comment moves ahead in UPDATED_AT order.
// These fixtures exercise our cutoff and cursor reuse with the documented ordering.
#[tokio::test]
async fn conversation_comments_resume_one_walk_across_chunks_and_select_by_creation() {
    const OLDER_START: &str = "2026-10-01T00:00:00Z";
    const OCT_10: &str = "2026-10-10T00:00:00Z";
    const OCT_03: &str = "2026-10-03T00:00:00Z";
    let server = TestServer::start(|request| {
        let body = request_body(request);
        let query = body["query"].as_str().unwrap();
        match query {
            ISSUE_COMMENTS if body["variables"]["after"].is_null() => conversations(
                vec![
                    conversation("outside-end", END, END, false, "Later"),
                    conversation("edited", OCT_10, IN_RANGE, false, "   Earlier  "),
                    conversation("pull-comment", START, START, true, "   "),
                    conversation("older-page-tail", OCT_10, OCT_10, false, "Memoized"),
                ],
                Some("older-page"),
            ),
            ISSUE_COMMENTS => {
                assert_eq!(body["variables"]["after"], "older-page");
                conversations(
                    vec![
                        conversation(
                            "older-page-head",
                            OCT_03,
                            OCT_03,
                            true,
                            "🦀".repeat(2_001).as_str(),
                        ),
                        conversation(
                            "before-range",
                            "2026-09-30T00:00:00Z",
                            "2026-09-30T00:00:00Z",
                            false,
                            "Too early",
                        ),
                    ],
                    Some("do-not-fetch"),
                )
            }
            commits::REPOSITORIES => answer(commit_discovery(&[], 0)),
            _ => standard_query(query).expect("unexpected query"),
        }
    });

    let api = api(&server);
    let state = GitHubCollection::default();
    let connection = ConnectionId::new(Source::Github, "example");
    let walk = state.conversations(&connection);
    let recent = super::collect(&api, options(), &walk).await.unwrap();
    let [comment] = recent.as_slice() else {
        panic!("expected one recent comment");
    };
    assert_eq!(comment.id, "pull-comment");
    assert!(matches!(
        &comment.action,
        GitHubAction::CommentedOn { target: CommentTarget::PullRequest(pull), body: None }
            if pull.number == 12
    ));
    assert_eq!(comment_requests(&server).len(), 1);

    let older = super::collect(
        &api,
        CollectOptions {
            start: OLDER_START.parse().unwrap(),
            end: START.parse().unwrap(),
        },
        &state.conversations(&connection),
    )
    .await
    .unwrap();
    let [first, edited, tail] = older.as_slice() else {
        panic!("expected three earlier comments");
    };
    assert_eq!(first.id, "older-page-head");
    assert!(matches!(
        &first.action,
        GitHubAction::CommentedOn { body: Some(body), .. }
            if body == &"🦀".repeat(2_001)
    ));
    assert_eq!(edited.id, "edited");
    assert_eq!(edited.timestamp.as_str(), OCT_10);
    assert!(matches!(
        &edited.action,
        GitHubAction::CommentedOn { target: CommentTarget::Issue(issue), body: Some(body) }
            if issue.number == 7 && body == "Earlier"
    ));
    assert_eq!(tail.id, "older-page-tail");
    assert_eq!(
        comment_requests(&server),
        [Value::Null, json!("older-page")]
    );
}

fn comment_requests(server: &TestServer) -> Vec<Value> {
    server
        .requests()
        .iter()
        .map(request_body)
        .filter(|body| body["query"] == ISSUE_COMMENTS)
        .map(|body| body["variables"]["after"].clone())
        .collect()
}

#[tokio::test]
async fn conversation_comments_skip_refused_rows_and_warn_once_across_chunks() {
    const EARLIER: &str = "2026-10-20T00:00:00Z";
    let server = TestServer::start(|request| {
        let body = request_body(request);
        match body["query"].as_str().unwrap() {
            ISSUE_COMMENTS => {
                let (rows, after) = if body["variables"]["after"].is_null() {
                    (
                        vec![
                            conversation("readable", START, START, false, "Read this"),
                            conversation("refused", START, START, false, "Omit this"),
                            conversation(
                                "previous-chunk",
                                EARLIER,
                                EARLIER,
                                false,
                                "Read this later",
                            ),
                        ],
                        Some("older-page"),
                    )
                } else {
                    assert_eq!(body["variables"]["after"], "older-page");
                    (vec![Value::Null], None)
                };
                let index = if after.is_some() { 1 } else { 0 };
                let errors: Vec<_> = ["repository", "url"]
                    .into_iter()
                    .map(|field| {
                        json!({
                            "type": "FORBIDDEN", "message": "Example refusal",
                            "path": ["viewer", "issueComments", "nodes", index, "issue", field]
                        })
                    })
                    .collect();
                Reply::json(
                    200,
                    json!({
                        "data": { "viewer": { "issueComments": page(rows, after) } },
                        "errors": errors,
                    })
                    .to_string(),
                )
            }
            commits::REPOSITORIES => answer(commit_discovery(&[], 0)),
            query => standard_query(query).expect("unexpected query"),
        }
    });
    let connection = ConnectionId::new(Source::Github, "refused-conversation-rows");
    let scope = connection.to_string();
    let api = api(&server);
    {
        let state = GitHubCollection::default();
        let walk = state.conversations(&connection);
        let recent = super::collect(&api, options(), &walk).await.unwrap();
        assert_eq!(
            recent.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
            ["readable"]
        );
        let earlier = super::collect(
            &api,
            CollectOptions {
                start: EARLIER.parse().unwrap(),
                end: START.parse().unwrap(),
            },
            &walk,
        )
        .await
        .unwrap();
        assert_eq!(
            earlier
                .iter()
                .map(|row| row.id.as_str())
                .collect::<Vec<_>>(),
            ["previous-chunk"]
        );
    }
    assert_eq!(
        comment_requests(&server),
        [Value::Null, json!("older-page")]
    );
    let warnings: Vec<_> = diagnostics::entries()
        .into_iter()
        .filter(|line| line.scope == scope && line.level == LogLevel::Warn)
        .map(|line| line.message)
        .collect();
    assert_eq!(
        warnings,
        ["Skipped 2 GitHub conversation comments the token could not read"]
    );
}

#[tokio::test]
async fn conversation_comment_refusals_without_a_readable_page_stay_typed() {
    for (kind, code, path, data) in [
        (
            QueryErrorKind::Forbidden,
            "FORBIDDEN",
            json!(["viewer"]),
            Value::Null,
        ),
        (
            QueryErrorKind::Forbidden,
            "FORBIDDEN",
            json!(["viewer", "issueComments"]),
            json!({"viewer": null}),
        ),
        (
            QueryErrorKind::Forbidden,
            "FORBIDDEN",
            json!(["viewer", "issueComments", "nodes", 0]),
            Value::Null,
        ),
        (
            QueryErrorKind::RateLimited,
            "RATE_LIMITED",
            json!(["viewer", "issueComments", "nodes", 0]),
            json!({"viewer": {"issueComments": page(vec![Value::Null], None)}}),
        ),
    ] {
        let server = TestServer::start(move |request| {
            let body = request_body(request);
            match body["query"].as_str().unwrap() {
                ISSUE_COMMENTS => Reply::json(
                    200,
                    json!({
                        "data": data,
                        "errors": [{"type": code, "message": "Example refusal", "path": path}],
                    })
                    .to_string(),
                ),
                commits::REPOSITORIES => answer(commit_discovery(&[], 0)),
                query => standard_query(query).expect("unexpected query"),
            }
        });
        let error = collect(&api(&server), options()).await.unwrap_err();
        let remote = error.downcast_ref::<RemoteFailure>().unwrap();
        let ApiError::Query(refusal) = &remote.error else {
            panic!("expected a typed GraphQL refusal");
        };
        assert!(refusal.kinds().eq([kind]));
    }
}

fn inline(id: &str, created_at: &str) -> Value {
    json!({
        "id": id, "createdAt": created_at, "body": "  Check this  ", "path": "src/main.rs",
    })
}

#[tokio::test]
async fn draft_comments_belong_to_the_review_submission_day_even_on_separate_fetches() {
    const DRAFT_DAY: &str = "2026-11-10T00:00:00Z";
    const CREATED: &str = "2026-11-10T20:00:00Z";
    const SUBMISSION_DAY: &str = "2026-11-11T00:00:00Z";
    const SUBMITTED: &str = "2026-11-11T02:00:00Z";
    const END: &str = "2026-11-12T00:00:00Z";
    let server = TestServer::start(|request| {
        let body = request_body(request);
        let mut row = review("review-example", "COMMENTED", Some(SUBMITTED));
        row["comments"] = page(vec![inline("inline-first", CREATED)], Some("inline-next"));
        let pull = pull("pr-example", 12, SUBMITTED, page(vec![row], None));
        match body["query"].as_str().unwrap() {
            REVIEWED_PULL_REQUESTS if body["variables"]["from"] == SUBMISSION_DAY => {
                answer(contributions(vec![json!({"pullRequest": pull})], None))
            }
            OWN_PULL_REQUESTS => answer(json!({"viewer": {"page": page(vec![pull], None)}})),
            MORE_REVIEW_COMMENTS => {
                assert_eq!(
                    body["variables"],
                    json!({"id": "review-example", "after": "inline-next"})
                );
                answer(json!({"node": {"id": "review-example", "comments": page(
                    vec![inline("inline-second", CREATED)], None
                )}}))
            }
            commits::REPOSITORIES => answer(commit_discovery(&[], 0)),
            query => standard_query(query).expect("unexpected query"),
        }
    });
    let api = api(&server);
    let draft_day = collect(
        &api,
        CollectOptions {
            start: DRAFT_DAY.parse().unwrap(),
            end: SUBMISSION_DAY.parse().unwrap(),
        },
    )
    .await
    .unwrap();
    assert!(draft_day.is_empty());
    assert!(
        server
            .requests()
            .iter()
            .all(|req| request_body(req)["query"] != MORE_REVIEW_COMMENTS)
    );

    let submission_day = collect(
        &api,
        CollectOptions {
            start: SUBMISSION_DAY.parse().unwrap(),
            end: END.parse().unwrap(),
        },
    )
    .await
    .unwrap();
    assert_eq!(
        submission_day
            .iter()
            .map(|e| e.id.as_str())
            .collect::<Vec<_>>(),
        ["review-example", "inline-first", "inline-second"],
    );
    for event in &submission_day[1..] {
        assert_eq!(event.timestamp.as_str(), SUBMITTED);
        assert!(matches!(
            &event.action,
            GitHubAction::CommentedOn {
                target: CommentTarget::ReviewLine { pull_request, path },
                body: Some(body),
            } if pull_request.number == 12 && path == "src/main.rs" && body == "Check this"
        ));
    }
}

#[tokio::test]
async fn inline_continuation_uses_repository_refusal_policy() {
    for kind in [QueryErrorKind::Forbidden, QueryErrorKind::RateLimited] {
        let code = match kind {
            QueryErrorKind::Forbidden => "FORBIDDEN",
            QueryErrorKind::RateLimited => "RATE_LIMITED",
            _ => unreachable!(),
        };
        let server = TestServer::start(move |request| {
            let body = request_body(request);
            match body["query"].as_str().unwrap() {
                REVIEWED_PULL_REQUESTS => {
                    let mut row = review("review-example", "COMMENTED", Some(IN_RANGE));
                    row["comments"] =
                        page(vec![inline("inline-first", IN_RANGE)], Some("inline-next"));
                    answer(contributions(
                        vec![json!({"pullRequest": pull(
                            "pr-example", 12, IN_RANGE, page(vec![row], None)
                        )})],
                        None,
                    ))
                }
                MORE_REVIEW_COMMENTS => Reply::json(
                    200,
                    json!({
                        "data": { "node": null },
                        "errors": [{
                            "type": code,
                            "message": "Example refusal",
                            "path": ["node", "comments"]
                        }]
                    })
                    .to_string(),
                ),
                commits::REPOSITORIES => answer(commit_discovery(&[], 0)),
                query => standard_query(query).expect("unexpected query"),
            }
        });
        let result = collect(&api(&server), options()).await;
        if kind == QueryErrorKind::Forbidden {
            let entries = result.unwrap();
            assert_eq!(
                entries
                    .iter()
                    .map(|entry| entry.id.as_str())
                    .collect::<Vec<_>>(),
                ["review-example", "inline-first"]
            );
        } else {
            let failure = result.unwrap_err().downcast::<RemoteFailure>().unwrap();
            assert!(
                matches!(failure.error, ApiError::Query(refusal) if refusal.kinds().eq([kind]))
            );
        }
    }
}

#[tokio::test]
async fn an_older_chunk_can_fill_the_memo_before_a_newer_chunk() {
    const EARLIER: &str = "2026-10-20T00:00:00Z";
    let server = TestServer::start(|request| {
        let body = request_body(request);
        match body["query"].as_str().unwrap() {
            ISSUE_COMMENTS if body["variables"]["after"].is_null() => conversations(
                vec![
                    conversation("newer", IN_RANGE, IN_RANGE, true, "New"),
                    conversation("older", EARLIER, EARLIER, false, "Old"),
                ],
                Some("boundary"),
            ),
            ISSUE_COMMENTS => {
                assert_eq!(body["variables"]["after"], "boundary");
                conversations(
                    vec![conversation(
                        "same-instant",
                        EARLIER,
                        EARLIER,
                        false,
                        "Also old",
                    )],
                    None,
                )
            }
            commits::REPOSITORIES => answer(commit_discovery(&[], 0)),
            query => standard_query(query).expect("unexpected query"),
        }
    });
    let api = api(&server);
    let walk = ConversationRun::default();
    let older = super::collect(
        &api,
        CollectOptions {
            start: EARLIER.parse().unwrap(),
            end: START.parse().unwrap(),
        },
        &walk,
    )
    .await
    .unwrap();
    assert_eq!(
        older.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
        ["older", "same-instant"],
    );
    let newer = super::collect(&api, options(), &walk).await.unwrap();
    assert_eq!(newer.len(), 1);
    assert_eq!(newer[0].id, "newer");
    assert_eq!(comment_requests(&server), [Value::Null, json!("boundary")]);
}

#[tokio::test]
async fn a_comment_order_failure_discards_the_memo_and_restarts_at_the_first_page() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    const EARLIER: &str = "2026-10-20T00:00:00Z";
    for (created, updated) in [(IN_RANGE, START), (IN_RANGE, IN_RANGE)] {
        let walks = AtomicUsize::new(0);
        let server = TestServer::start(move |request| {
            let body = request_body(request);
            match body["query"].as_str().unwrap() {
                ISSUE_COMMENTS if body["variables"]["after"].is_null() => {
                    if walks.fetch_add(1, Ordering::Relaxed) == 0 {
                        conversations(
                            vec![conversation("partial", START, START, false, "Discard")],
                            Some("broken"),
                        )
                    } else {
                        conversations(
                            vec![conversation("fresh", EARLIER, EARLIER, false, "Keep")],
                            None,
                        )
                    }
                }
                ISSUE_COMMENTS => {
                    assert_eq!(body["variables"]["after"], "broken");
                    conversations(
                        vec![conversation(
                            "bad-order",
                            created,
                            updated,
                            false,
                            "Invalid",
                        )],
                        None,
                    )
                }
                commits::REPOSITORIES => answer(commit_discovery(&[], 0)),
                query => standard_query(query).expect("unexpected query"),
            }
        });
        let api = api(&server);
        let walk = ConversationRun::default();
        let error = super::collect(&api, options(), &walk).await.unwrap_err();
        assert_eq!(
            error.downcast_ref::<CollectionError>(),
            Some(&CollectionError::CommentOrder)
        );
        let older = super::collect(
            &api,
            CollectOptions {
                start: EARLIER.parse().unwrap(),
                end: START.parse().unwrap(),
            },
            &walk,
        )
        .await
        .unwrap();
        assert_eq!(older.len(), 1);
        assert_eq!(older[0].id, "fresh");
        assert_eq!(
            comment_requests(&server),
            [Value::Null, json!("broken"), Value::Null]
        );
    }
}
