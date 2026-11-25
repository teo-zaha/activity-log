use std::{sync::Arc, time::Duration};

use serde_json::{Value, json};
use tokio::time::Instant;

use super::{Budget, GitHubApi, GitHubChanges, GitHubConfig, Wanted, change_ref, native_then_walk};
use crate::{
    collect::change_stack::{ChangeLookup, FailureKind, LookupCause, STACK_DEPTH},
    model::{
        ActivityEntry, ChangeStackBottom, ChangeStackEnd, CollectOptions, ConnectionId, LogLevel,
        Source, StackCut,
    },
    provider::{
        ProviderRuntime,
        connection::{Connection, ConnectionLocation, ConnectionValues},
        github::GitHubProvider,
        remote_api::test_server::{Reply, Request, TestServer},
        remote_error::{ApiError, OAuthRefusal, QueryAnswer},
    },
    secrets::Secrets,
};

use super::{
    lookup::request_failure,
    native::{NativePull, unsupported},
};

const REPO: &str = "acme/widgets";
const AT: &str = "2026-11-12T09:00:00Z";

fn api(server: &TestServer) -> GitHubApi {
    GitHubApi::new(&GitHubConfig {
        url: server.url.clone(),
        token: "example-token".into(),
    })
    .unwrap()
}

fn wanted(number: u64) -> Wanted {
    (change_ref(REPO, number).unwrap(), AT.parse().unwrap())
}

fn body(request: &Request) -> Value {
    assert_eq!(request.method, "POST");
    assert_eq!(request.path(), "/api/graphql");
    assert_eq!(
        request.header("authorization"),
        Some("Bearer example-token")
    );
    serde_json::from_slice(&request.body).unwrap()
}

fn reply(body: Value) -> Reply {
    Reply::json(200, body.to_string())
}

fn repository(fields: Value) -> Value {
    let mut repository = fields;
    repository["nameWithOwner"] = json!("Acme/Widgets");
    json!({"repository": repository})
}

fn pull(number: u64, state: &str, head: &str, base: &str) -> Value {
    json!({
        "number": number, "id": format!("PR_{number}"), "title": format!("Widgets {number}"),
        "state": state, "headRefName": head, "baseRefName": base, "isCrossRepository": false,
        "url": format!("https://links.example.org/acme/widgets/pull/{number}"),
    })
}

fn list(nodes: Vec<Value>, next: Option<&str>) -> Reply {
    reply(json!({"data": repository(json!({"pullRequests": {
        "nodes": nodes, "pageInfo": {"hasNextPage": next.is_some(), "endCursor": next}
    }}))}))
}

fn membership(pull: &mut Value, position: u32, nodes: Vec<Value>, more: bool) {
    pull["stackEntry"] = json!({
        "position": position,
        "stack": {"baseRefName":"main", "entries": {"nodes":nodes, "pageInfo":{"hasNextPage":more}}},
    });
}

fn opened(number: u64) -> ActivityEntry {
    serde_json::from_value(json!({
        "source":"github", "connection":"work", "id":format!("PR_{number}"), "timestamp":AT,
        "repository":REPO, "type":"opened", "target":{"kind":"pullRequest","number":number,"title":"Old title"},
    })).unwrap()
}

async fn runtime(server: &TestServer, name: &str) -> ProviderRuntime {
    let connection = Connection::new(
        ConnectionId::new(Source::Github, name),
        name,
        ConnectionLocation::Local,
        ConnectionValues::new(
            [
                ("GITHUB_URL".into(), Some(server.url.clone())),
                ("GITHUB_TOKEN".into(), Some("example-token".into())),
            ]
            .into_iter()
            .collect(),
        ),
    );
    let runtime =
        ProviderRuntime::inspect(Arc::new(GitHubProvider), &connection, &Secrets::none()).await;
    assert!(runtime.usable());
    runtime
}

#[tokio::test]
async fn native_annotation_uses_open_positions_and_walks_from_both_ends() {
    let server = TestServer::start(|request| {
        if request.method == "GET" {
            return reply(json!({"id": 7, "login": "jdoe"}));
        }
        let body = body(request);
        let query = body["query"].as_str().unwrap();
        if query.starts_with("query OpenStackPulls(") {
            let vars = &body["variables"];
            return match (vars["head"].as_str(), vars["base"].as_str()) {
                (Some("platform"), None) => list(vec![pull(1, "OPEN", "platform", "main")], None),
                (None, Some("finish")) => {
                    list(vec![pull(7, "OPEN", "branch-child", "finish")], None)
                }
                (Some("main"), None) | (None, Some("branch-child")) => list(Vec::new(), None),
                _ => panic!("unexpected branch lookup {vars}"),
            };
        }
        assert!(query.starts_with("query NativeStackNumbers("));
        assert!(query.contains("stackEntry"));
        assert!(!query.contains(REPO));
        assert_eq!(
            body["variables"],
            json!({"owner":"acme","name":"widgets","n0":4,"n1":8})
        );
        let host = request.header("host").unwrap();
        let mut parent = pull(2, "OPEN", "foundation", "main");
        parent["url"] = json!(format!("http://{host}/acme/widgets/pull/2"));
        let mut current = pull(4, "OPEN", "next", "foundation");
        membership(
            &mut current,
            3,
            vec![
                json!({"position":4,"pullRequest":pull(6,"OPEN","finish","next")}),
                json!({"position":2,"pullRequest":pull(3,"MERGED","merged","foundation")}),
                json!({"position":1,"pullRequest":parent}),
                json!({"position":3,"pullRequest":pull(4,"OPEN","next","foundation")}),
            ],
            false,
        );
        current["stackEntry"]["stack"]["baseRefName"] = json!("platform");
        let mut merged = pull(8, "MERGED", "done", "main");
        merged["stackEntry"] = Value::Null;
        reply(json!({"data":repository(json!({"n0":current,"n1":merged}))}))
    });
    let runtime = runtime(&server, "native-annotation").await;
    let review = serde_json::from_value(json!({
        "source":"github","connection":"work","id":"PRR_example","timestamp":AT,
        "repository":REPO,"type":"reviewed","pullRequest":{"number":4,"title":"Old title"},"state":"approved"
    })).unwrap();
    let mut merged = serde_json::to_value(opened(8)).unwrap();
    merged["target"]["changeStack"] = json!({
        "current":{"number":8,"title":"Stale"},"below":[],
        "bottom":{"kind":"cut","reason":"limit"},"above":{"changes":[],"end":{"kind":"complete"}}
    });
    let mut entries = [opened(4), review, serde_json::from_value(merged).unwrap()];
    runtime.annotate(&mut entries).await;
    let [opened, reviewed, closed] = entries.each_mut().map(|entry| {
        let ActivityEntry::GitHub(entry) = entry else {
            panic!("expected GitHub")
        };
        entry.action.pull_request_mut().unwrap()
    });
    let stack = opened.change_stack.as_ref().unwrap();
    assert_eq!(reviewed.change_stack.as_ref(), Some(stack));
    assert!(closed.change_stack.is_none());
    assert_eq!(stack.current().number, 4);
    assert_eq!(stack.current().web_url, None);
    assert_eq!(
        stack
            .below()
            .iter()
            .map(|pull| pull.number)
            .collect::<Vec<_>>(),
        [1, 2]
    );
    assert!(stack.below()[1].web_url.is_some());
    assert_eq!(
        stack.bottom(),
        &ChangeStackBottom::Reached {
            branch: "main".into()
        }
    );
    let child = &stack.above().changes[0];
    assert_eq!(child.change.number, 6);
    assert_eq!(child.above.changes[0].change.number, 7);
    assert_eq!(child.above.changes[0].above.end, ChangeStackEnd::Complete);
    assert_eq!(server.requests().len(), 6);
    let warnings: Vec<_> = crate::diagnostics::entries()
        .into_iter()
        .filter(|line| line.scope == "github/native-annotation" && line.level == LogLevel::Warn)
        .map(|line| line.message)
        .collect();
    assert_eq!(
        warnings,
        ["Omitted 4 change-stack links to another origin, first: https://links.example.org"]
    );
}

#[tokio::test]
async fn null_native_stack_and_typed_missing_field_fall_back_to_the_branch_walk() {
    for unsupported in [false, true] {
        let server = TestServer::start(move |request| {
            let body = body(request);
            let query = body["query"].as_str().unwrap();
            if query.starts_with("query NativeStackNumbers(") {
                return if unsupported {
                    reply(json!({"errors":[{
                        "extensions": {"code":"undefinedField"},
                        "path":["query NativeStackNumbers","repository","n0","stackEntry"],
                        "message":"Example schema response",
                    }]}))
                } else {
                    let mut current = pull(4, "OPEN", "next", "foundation");
                    current["stackEntry"] = json!({"position":1,"stack":null});
                    reply(json!({"data":repository(json!({"n0":current}))}))
                };
            }
            assert!(query.starts_with("query OpenStackPulls("));
            assert_eq!(body["variables"]["head"], Value::Null);
            assert_eq!(body["variables"]["base"], Value::Null);
            list(
                vec![
                    pull(2, "OPEN", "foundation", "main"),
                    pull(4, "OPEN", "next", "foundation"),
                ],
                None,
            )
        });
        let api = api(&server);
        let lookup = GitHubChanges::new(&api);
        let (key, _) = wanted(4);
        let stacks = native_then_walk(vec![wanted(4)], &lookup, &Budget::for_collection()).await;
        assert_eq!(stacks[&key].below()[0].number, 2);
        assert_eq!(
            stacks[&key].bottom(),
            &ChangeStackBottom::Reached {
                branch: "main".into()
            }
        );
        assert_eq!(server.requests().len(), 2);
    }
}

#[tokio::test]
async fn native_and_fallback_share_the_budget_and_busy_replies_are_not_retried() {
    for (limit, expected, requests) in [(2, StackCut::Limit, 2), (10, StackCut::Throttled, 3)] {
        let server = TestServer::start(move |request| {
            let body = body(request);
            let query = body["query"].as_str().unwrap();
            if query.starts_with("query NativeStackNumbers(") {
                let mut current = pull(4, "OPEN", "next", "foundation");
                current["stackEntry"] = Value::Null;
                return reply(json!({"data":repository(json!({"n0":current}))}));
            }
            if body["variables"]["head"].is_null() && body["variables"]["base"].is_null() {
                return list(vec![pull(4, "OPEN", "next", "foundation")], Some("more"));
            }
            Reply::json(429, "{}").with_header("Retry-After", "1")
        });
        let api = api(&server);
        let budget = Budget::new(limit, Instant::now() + Duration::from_secs(4));
        let (key, _) = wanted(4);
        let stacks = native_then_walk(vec![wanted(4)], &GitHubChanges::new(&api), &budget).await;
        assert_eq!(
            stacks[&key].bottom(),
            &ChangeStackBottom::Cut { reason: expected }
        );
        assert_eq!(
            stacks[&key].above().end,
            ChangeStackEnd::Cut { reason: expected }
        );
        assert_eq!(server.requests().len(), requests);
    }
}

#[tokio::test]
async fn a_native_first_page_with_more_entries_leaves_a_limit_cut() {
    let server = TestServer::start(|request| {
        let body = body(request);
        if body["query"]
            .as_str()
            .unwrap()
            .starts_with("query OpenStackPulls(")
        {
            assert_eq!(body["variables"]["head"], "main");
            assert_eq!(body["variables"]["base"], Value::Null);
            return list(Vec::new(), None);
        }
        assert!(
            body["query"]
                .as_str()
                .unwrap()
                .starts_with("query NativeStackNumbers(")
        );
        let mut current = pull(4, "OPEN", "next", "main");
        membership(
            &mut current,
            1,
            vec![
                json!({"position":1,"pullRequest":pull(4,"OPEN","next","main")}),
                json!({"position":2,"pullRequest":pull(6,"OPEN","finish","next")}),
            ],
            true,
        );
        reply(json!({"data":repository(json!({"n0":current}))}))
    });
    let api = api(&server);
    let (key, _) = wanted(4);
    let stacks = native_then_walk(
        vec![wanted(4)],
        &GitHubChanges::new(&api),
        &Budget::for_collection(),
    )
    .await;
    assert_eq!(
        stacks[&key].above().changes[0].above.end,
        ChangeStackEnd::Cut {
            reason: StackCut::Limit
        }
    );
    assert_eq!(server.requests().len(), 2);
}

#[tokio::test]
async fn number_aliases_ignore_only_a_not_found_for_the_requested_pull() {
    let server = TestServer::start(|request| {
        let body = body(request);
        assert!(
            body["query"]
                .as_str()
                .unwrap()
                .contains("n1: pullRequest(number: $n1)")
        );
        assert_eq!(body["variables"]["n1"], 7);
        reply(json!({
            "data":repository(json!({"n0":pull(4,"OPEN","next","main"),"n1":null,"n2":false})),
            "errors":[{"type":"NOT_FOUND","path":["repository","n1"]}],
        }))
    });
    let api = api(&server);
    let lookup = GitHubChanges::new(&api);
    let (key, _) = wanted(4);
    let budget = Budget::for_collection();
    let rows = lookup
        .open_numbers(&key.repo, &[4, 7, 8], &budget)
        .await
        .unwrap();
    assert_eq!(rows.iter().map(|row| row.number()).collect::<Vec<_>>(), [4]);

    let error = lookup
        .open_numbers(&key.repo, &[u64::MAX], &budget)
        .await
        .unwrap_err();
    assert_eq!(error.cause(), Some(&LookupCause::IgnoredFilter));
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn branch_lists_page_by_cursor_with_exact_filters_and_reject_mismatches() {
    let branch = "feature/quoted";
    let server = TestServer::start(move |request| {
        let body = body(request);
        let variables = &body["variables"];
        assert!(!body["query"].as_str().unwrap().contains(branch));
        if variables["base"] == branch {
            return list(vec![pull(4, "OPEN", "finish", "other")], None);
        }
        assert_eq!(variables["head"], branch);
        if variables["after"].is_null() {
            list(vec![pull(3, "OPEN", branch, "main")], Some("cursor"))
        } else {
            assert_eq!(variables["after"], "cursor");
            list(vec![pull(4, "OPEN", branch, "main")], None)
        }
    });
    let api = api(&server);
    let lookup = GitHubChanges::new(&api);
    let (key, _) = wanted(4);
    let budget = Budget::for_collection();
    let rows = lookup.open_from(&key.repo, branch, &budget).await.unwrap();
    assert_eq!(
        rows.iter().map(|row| row.number()).collect::<Vec<_>>(),
        [3, 4]
    );
    let error = lookup
        .open_into(&key.repo, branch, &budget)
        .await
        .unwrap_err();
    assert_eq!(error.kind(), FailureKind::Failed);
    assert_eq!(error.cause(), Some(&LookupCause::IgnoredFilter));
    assert_eq!(server.requests().len(), 3);
}

#[tokio::test]
async fn native_positions_cut_at_the_shared_depth_in_both_directions() {
    let current = STACK_DEPTH as u64 + 1;
    let server = TestServer::start(move |request| {
        let body = body(request);
        assert!(
            body["query"]
                .as_str()
                .unwrap()
                .starts_with("query NativeStackNumbers(")
        );
        let nodes = (1..=current + STACK_DEPTH as u64).map(|number| {
            json!({"position": number, "pullRequest":pull(number, "OPEN", "feature", "main")})
        }).collect();
        let mut row = pull(current, "OPEN", "feature", "main");
        membership(&mut row, current as u32, nodes, false);
        reply(json!({"data":repository(json!({"n0":row}))}))
    });
    let api = api(&server);
    let (key, _) = wanted(current);
    let stacks = native_then_walk(
        vec![wanted(current)],
        &GitHubChanges::new(&api),
        &Budget::for_collection(),
    )
    .await;
    let stack = &stacks[&key];
    assert_eq!(stack.below().len(), STACK_DEPTH);
    assert_eq!(
        stack.bottom(),
        &ChangeStackBottom::Cut {
            reason: StackCut::Limit
        }
    );
    let mut level = stack.above();
    for _ in 0..STACK_DEPTH {
        let [node] = level.changes.as_slice() else {
            panic!("expected one native successor")
        };
        level = &node.above;
    }
    assert!(level.changes.is_empty());
    assert_eq!(
        level.end,
        ChangeStackEnd::Cut {
            reason: StackCut::Limit
        }
    );
    assert_eq!(server.requests().len(), 1);
}

fn query_failure(errors: Value) -> ApiError {
    QueryAnswer::read(json!({"errors":errors}), &[])
        .unwrap()
        .complete::<Value>()
        .unwrap_err()
}

#[test]
fn stack_failures_use_busy_evidence_then_oauth_codes_then_http_status() {
    use FailureKind::{Failed, ScopeMissing, Throttled, TokenInvalid, Unreadable};
    use OAuthRefusal::{InsufficientScope, InvalidRequest, InvalidToken};

    for (status, refusal, retry_after, kind) in [
        (403, Some(InvalidToken), Some(1), Throttled),
        (403, Some(InsufficientScope), Some(1), Throttled),
        (401, Some(InsufficientScope), None, ScopeMissing),
        (404, Some(InvalidToken), None, TokenInvalid),
        (401, Some(InvalidRequest), None, Failed),
        (401, None, None, TokenInvalid),
        (403, None, None, TokenInvalid),
        (404, None, None, Failed),
    ] {
        let error = ApiError::Status {
            status: reqwest::StatusCode::from_u16(status).unwrap(),
            message: String::new(),
            retry_after,
            quota: None,
            refusal,
        };
        let failure = request_failure(error.clone());
        assert_eq!(failure.kind(), kind);
        assert_eq!(failure.cause(), Some(&LookupCause::Request(error)));
    }
    for (codes, kind) in [
        (&["INSUFFICIENT_SCOPES"][..], ScopeMissing),
        (&["INSUFFICIENT_SCOPES", "RATE_LIMITED"], Throttled),
        (&["FORBIDDEN", "NOT_FOUND"], Unreadable),
        (&["FORBIDDEN", "OTHER"], Failed),
    ] {
        let errors = Value::Array(codes.iter().map(|code| json!({"type":code})).collect());
        assert_eq!(request_failure(query_failure(errors)).kind(), kind);
    }
}

#[test]
fn unsupported_native_field_requires_a_matching_schema_refusal() {
    let absent = json!({
        "extensions":{"code":"undefinedField"},
        "path":["fragment NativeStackEntry","stackEntry"],
        "message":"Example schema refusal",
    });
    let other = json!({
        "extensions":{"code":"undefinedField"},
        "path":["fragment StackPull","headRefName"],
    });
    for (errors, fallback) in [
        (json!([absent]), true),
        (json!([other]), false),
        (
            json!([absent, {"type":"NOT_FOUND","path":["repository","n0"]}]),
            false,
        ),
    ] {
        assert_eq!(
            unsupported(&request_failure(query_failure(errors))),
            fallback
        );
    }
}

#[test]
fn invalid_native_rows_keep_their_lookup_failure_kind() {
    let server = TestServer::start(|_| panic!("native validation sends no requests"));
    let api = api(&server);
    let lookup = GitHubChanges::new(&api);
    let current = json!({"position":2,"pullRequest":pull(4,"OPEN","next","main")});
    let parent = json!({"position":1,"pullRequest":pull(2,"OPEN","parent","main")});
    for (positions, kind) in [
        (
            vec![
                current.clone(),
                parent.clone(),
                json!({"position":1,"pullRequest":pull(3,"OPEN","dup","main")}),
            ],
            FailureKind::Failed,
        ),
        (
            vec![
                current.clone(),
                parent.clone(),
                json!({"position":3,"pullRequest":pull(2,"OPEN","dup","main")}),
            ],
            FailureKind::Failed,
        ),
        (
            vec![
                parent.clone(),
                json!({"position":3,"pullRequest":pull(4,"OPEN","next","main")}),
            ],
            FailureKind::Failed,
        ),
        (
            vec![current.clone(), json!({"position":1,"pullRequest":null})],
            FailureKind::Unreadable,
        ),
    ] {
        let mut row = pull(4, "OPEN", "next", "main");
        membership(&mut row, 2, positions, false);
        let row: NativePull = serde_json::from_value(row).unwrap();
        let Err(failure) = row.stack(4, &lookup) else {
            panic!("expected a refused native shape")
        };
        assert_eq!(failure.kind(), kind);
        if kind == FailureKind::Failed {
            assert_eq!(failure.cause(), Some(&LookupCause::IgnoredFilter));
        }
    }
}

#[tokio::test]
async fn a_native_breaker_stops_later_batches_and_pending_walks() {
    for (status, expected_kind) in [
        (401, FailureKind::TokenInvalid),
        (403, FailureKind::TokenInvalid),
        (429, FailureKind::Throttled),
    ] {
        for first_succeeds in [false, true] {
            let server = TestServer::start(move |request| {
                let body = body(request);
                assert!(
                    body["query"]
                        .as_str()
                        .unwrap()
                        .starts_with("query NativeStackNumbers(")
                );
                let variables = body["variables"].as_object().unwrap();
                if first_succeeds && variables["n0"] == 1 {
                    let mut fields = serde_json::Map::new();
                    for (alias, number) in variables
                        .iter()
                        .filter(|(key, _)| key.starts_with('n') && *key != "name")
                    {
                        let mut row = pull(number.as_u64().unwrap(), "OPEN", "feature", "main");
                        row["stackEntry"] = Value::Null;
                        fields.insert(alias.clone(), row);
                    }
                    return reply(json!({"data":repository(Value::Object(fields))}));
                }
                let reply = Reply::json(status, "{}");
                if status == 429 {
                    reply.with_header("Retry-After", "1")
                } else {
                    reply
                }
            });
            let api = api(&server);
            let budget = Budget::for_collection();
            let connection = ConnectionId::new(
                Source::Github,
                format!("native-breaker-{status}-{first_succeeds}"),
            );
            let scope = connection.to_string();
            assert!(
                crate::diagnostics::in_connection(
                    connection,
                    native_then_walk(
                        (1..=51).map(wanted).collect(),
                        &GitHubChanges::new(&api),
                        &budget,
                    ),
                )
                .await
                .is_empty()
            );
            assert_eq!(budget.check().unwrap_err().kind(), expected_kind);
            assert_eq!(server.requests().len(), if first_succeeds { 2 } else { 1 });
            let warnings: Vec<_> = crate::diagnostics::entries()
                .into_iter()
                .filter(|line| line.scope == scope && line.level == LogLevel::Warn)
                .collect();
            assert_eq!(warnings.len(), 1);
            assert!(warnings[0].message.contains("51 changes in 1 repository"));
        }
    }
}

#[tokio::test]
async fn a_malformed_native_alias_leaves_other_pull_requests_available() {
    let server = TestServer::start(|request| {
        let body = body(request);
        if body["query"]
            .as_str()
            .unwrap()
            .starts_with("query NativeStackNumbers(")
        {
            let mut current = pull(4, "OPEN", "next", "main");
            current["stackEntry"] = Value::Null;
            return reply(json!({"data":repository(json!({"n0":false,"n1":current}))}));
        }
        list(
            vec![
                pull(2, "OPEN", "parent", "main"),
                pull(4, "OPEN", "next", "parent"),
            ],
            None,
        )
    });
    let api = api(&server);
    let connection = ConnectionId::new(Source::Github, "bad-native-alias");
    let stacks = crate::diagnostics::in_connection(
        connection,
        native_then_walk(
            vec![wanted(3), wanted(4)],
            &GitHubChanges::new(&api),
            &Budget::for_collection(),
        ),
    )
    .await;
    assert_eq!(stacks.len(), 1);
    assert_eq!(stacks[&wanted(4).0].below()[0].number, 2);
    let warnings: Vec<_> = crate::diagnostics::entries()
        .into_iter()
        .filter(|line| line.scope == "github/bad-native-alias" && line.level == LogLevel::Warn)
        .collect();
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].message.contains("whether 1 change"));
}

#[tokio::test]
async fn a_failed_native_lookup_keeps_the_entries_without_stacks() {
    let server = TestServer::start(|request| {
        if request.method == "GET" {
            reply(json!({"id":7,"login":"jdoe"}))
        } else {
            Reply::json(500, r#"{"message":"Example database failure"}"#)
        }
    });
    let runtime = runtime(&server, "failed-native-lookup").await;
    let mut entries = [opened(4), opened(6)];
    let before = serde_json::to_value(&entries).unwrap();
    runtime.annotate(&mut entries).await;
    assert_eq!(serde_json::to_value(entries).unwrap(), before);
    let warnings: Vec<_> = crate::diagnostics::entries()
        .into_iter()
        .filter(|line| line.scope == "github/failed-native-lookup" && line.level == LogLevel::Warn)
        .collect();
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].message.contains("Example database failure"));
    assert_eq!(server.requests().len(), 2);
}

#[tokio::test]
async fn merged_entries_use_no_stack_requests_while_reopened_closes_can_get_a_stack() {
    let server = TestServer::start(|request| {
        if request.method == "GET" {
            return reply(json!({"id": 7, "login": "jdoe"}));
        }
        let body = body(request);
        if body["query"]
            .as_str()
            .unwrap()
            .starts_with("query OpenStackPulls(")
        {
            return list(Vec::new(), None);
        }
        assert!(
            body["query"]
                .as_str()
                .unwrap()
                .starts_with("query NativeStackNumbers(")
        );
        let mut row = pull(4, "OPEN", "next", "parent");
        membership(
            &mut row,
            2,
            vec![
                json!({"position":1,"pullRequest":pull(2,"OPEN","parent","main")}),
                json!({"position":2,"pullRequest":pull(4,"OPEN","next","parent")}),
            ],
            false,
        );
        reply(json!({"data":repository(json!({"n0":row}))}))
    });
    let runtime = runtime(&server, "filtered-merged-entries").await;
    let mut closed = serde_json::to_value(opened(4)).unwrap();
    closed["type"] = json!("closed");
    let merged = |number| {
        serde_json::from_value(json!({
            "source":"github", "connection":"work", "id":format!("PR_{number}:merged"),
            "timestamp":AT, "repository":REPO, "type":"merged",
            "pullRequest":{"number":number,"title":"Merge example"}
        }))
        .unwrap()
    };
    let mut entries = [
        serde_json::from_value(closed).unwrap(),
        merged(4),
        merged(8),
    ];
    runtime.annotate(&mut entries).await;

    let native_queries: Vec<_> = server
        .requests()
        .iter()
        .filter(|request| request.method == "POST")
        .map(body)
        .filter(|body| {
            body["query"]
                .as_str()
                .unwrap()
                .starts_with("query NativeStackNumbers(")
        })
        .collect();
    assert_eq!(native_queries.len(), 1);
    assert_eq!(
        native_queries[0]["variables"],
        json!({"owner":"acme","name":"widgets","n0":4})
    );
    let annotated = serde_json::to_value(entries).unwrap();
    assert_eq!(
        annotated[0]["target"]["changeStack"]["below"][0]["number"],
        2
    );
    assert!(annotated[1]["pullRequest"].get("changeStack").is_none());
    assert!(annotated[2]["pullRequest"].get("changeStack").is_none());
}

#[tokio::test]
async fn native_annotations_stay_out_of_the_day_cache_and_reach_cached_entries() {
    let server = TestServer::start(|request| {
        if request.method == "GET" {
            return reply(json!({"id":7,"login":"jdoe"}));
        }
        let body = body(request);
        let operation = body["query"]
            .as_str()
            .unwrap()
            .split_whitespace()
            .nth(1)
            .unwrap();
        let operation = operation.split('(').next().unwrap();
        let page = |nodes: Vec<Value>| json!({"pageInfo":{"hasNextPage":false,"endCursor":null},"nodes":nodes});
        match operation {
            "ActivityViewer" => reply(json!({"data":{"viewer":{"id":"U_example","login":"jdoe"}}})),
            "OpenedPullRequests" | "OpenedIssues" | "ReviewedPullRequests" => {
                let nodes = if operation == "OpenedPullRequests" {
                    vec![json!({"occurredAt":AT,"item":{
                        "id":"PR_4","number":4,"title":"Old title","repository":{"nameWithOwner":REPO}
                    }})]
                } else {
                    Vec::new()
                };
                reply(json!({"data":{"viewer":{"contributionsCollection":{
                    "restrictedContributionsCount":0,"page":page(nodes)
                }}}}))
            }
            "OwnPullRequests" => reply(json!({"data":{"viewer":{"page":page(Vec::new())}}})),
            "StateChanges" => {
                let mut empty = page(Vec::new());
                empty["issueCount"] = json!(0);
                reply(json!({"data":{
                    "mergeInvolved":empty,"mergeReviewed":empty,"closePulls":empty,"closeIssues":empty
                }}))
            }
            "ViewerIssueComments" => {
                let target = json!({
                    "id":"PR_4","number":4,"title":"Old title","repository":{"nameWithOwner":REPO}
                });
                reply(json!({"data":{"viewer":{"issueComments":page(vec![json!({
                    "id":"IC_example","createdAt":AT,"updatedAt":AT,"body":"Example comment",
                    "pullRequest":target,"issue":target
                })])}}}))
            }
            "CommitRepositories" => reply(json!({"data":{"viewer":{"contributionsCollection":{
                "restrictedContributionsCount":0,"totalRepositoriesWithContributedCommits":0,
                "commitContributionsByRepository":[]
            }}}})),
            "NativeStackNumbers" => {
                let mut row = pull(4, "OPEN", "next", "parent");
                membership(
                    &mut row,
                    2,
                    vec![
                        json!({"position":1,"pullRequest":pull(2,"OPEN","parent","main")}),
                        json!({"position":2,"pullRequest":pull(4,"OPEN","next","parent")}),
                    ],
                    false,
                );
                reply(json!({"data":repository(json!({"n0":row}))}))
            }
            "OpenStackPulls" => list(Vec::new(), None),
            _ => panic!("unexpected query {operation}"),
        }
    });
    let runtime = runtime(&server, "stack-cache").await;
    let dir = tempfile::tempdir().unwrap();
    let range = CollectOptions {
        start: "2026-11-12T00:00:00Z".parse().unwrap(),
        end: "2026-11-13T00:00:00Z".parse().unwrap(),
    };
    let mut first = runtime.collect_in(dir.path(), range, None).await.unwrap();
    runtime.annotate(&mut first).await;
    assert_eq!(first.len(), 2);
    let cached_text: String = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|file| std::fs::read_to_string(file.unwrap().path()).unwrap_or_default())
        .collect();
    assert!(cached_text.contains("PR_4"));
    assert!(cached_text.contains("IC_example"));
    assert!(!cached_text.contains("changeStack"));
    let mut cached = runtime.collect_in(dir.path(), range, None).await.unwrap();
    let stored = serde_json::to_value(&cached).unwrap();
    for event in stored.as_array().unwrap() {
        assert!(event["target"].get("changeStack").is_none());
    }
    runtime.annotate(&mut cached).await;
    let first = serde_json::to_value(first).unwrap();
    for event in first.as_array().unwrap() {
        assert_eq!(event["target"]["changeStack"]["below"][0]["number"], 2);
    }
    assert_eq!(serde_json::to_value(cached).unwrap(), first);
    let requests = server.requests();
    for (query, count) in [
        ("query OpenedPullRequests", 1),
        ("query ViewerIssueComments", 1),
        ("query NativeStackNumbers", 2),
    ] {
        assert_eq!(
            requests
                .iter()
                .filter(|request| request.method == "POST"
                    && body(request)["query"].as_str().unwrap().starts_with(query))
                .count(),
            count
        );
    }
}
