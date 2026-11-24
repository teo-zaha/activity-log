use serde_json::{Value, json};

use super::*;
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
    json!({ "id": id, "state": state, "submittedAt": timestamp, "author": { "login": "jdoe" } })
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
            VIEWER => answer(json!({ "viewer": { "login": "jdoe" } })),
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
    assert_eq!(server.requests().len(), 7);
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
            return answer(json!({ "viewer": { "login": "jdoe" } }));
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
        matches!(remote.error, ApiError::Query(refusal) if refusal.kinds() == [QueryErrorKind::RateLimited])
    );
    assert_eq!(server.requests().len(), 2);
}

#[tokio::test]
async fn a_repeated_cursor_stops_before_it_is_requested_again() {
    let server = TestServer::start(|request| {
        let body = request_body(request);
        if body["query"] == VIEWER {
            return answer(json!({ "viewer": { "login": "jdoe" } }));
        }
        assert_eq!(body["query"], OPENED_PULL_REQUESTS);
        answer(contributions(vec![], Some("same-cursor")))
    });
    let error = collect(&api(&server), options()).await.unwrap_err();
    assert_eq!(
        error.downcast::<CollectionError>().unwrap(),
        CollectionError::RepeatedPage
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
        CollectionError::MissingNextCursor,
        CollectionError::ReviewPullRequestMismatch,
        CollectionError::ReviewAuthorMismatch,
    ] {
        let server = TestServer::start(move |request| {
            let body = request_body(request);
            match body["query"].as_str().unwrap() {
                VIEWER => answer(json!({ "viewer": { "login": "jdoe" } })),
                OPENED_PULL_REQUESTS | OPENED_ISSUES => answer(contributions(vec![], None)),
                REVIEWED_PULL_REQUESTS => {
                    let reviews = match expected {
                        CollectionError::MissingNextCursor => json!({
                            "pageInfo": {"hasNextPage": true, "endCursor": null}, "nodes": []
                        }),
                        CollectionError::ReviewPullRequestMismatch => page(vec![], Some("second")),
                        CollectionError::ReviewAuthorMismatch => {
                            let mut row = review("review-a", "APPROVED", Some(IN_RANGE));
                            row["author"]["login"] = json!("other-example-user");
                            page(vec![row], None)
                        }
                        CollectionError::RepeatedPage => unreachable!("checked separately"),
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
