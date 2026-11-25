use super::super::state_changes::{
    CLOSE_REQUESTS_PER_WINDOW, MORE_CLOSE_EVENTS, SEARCH, SEARCH_PAGES, SEARCH_REQUESTS_PER_WINDOW,
    SEARCHES,
};
use super::*;

fn search_page(count: u64, nodes: Vec<Value>, after: Option<&str>) -> Value {
    let mut result = page(nodes, after);
    result["issueCount"] = json!(count);
    result
}

fn searches(involved: Value, reviewed: Value, pulls: Value, issues: Value) -> Reply {
    answer(json!({
        "mergeInvolved": involved, "mergeReviewed": reviewed, "closePulls": pulls, "closeIssues": issues,
    }))
}

pub(super) fn no_state_changes() -> Reply {
    let empty = search_page(0, vec![], None);
    searches(empty.clone(), empty.clone(), empty.clone(), empty)
}

fn actor(id: &str) -> Value {
    json!({"__typename": "User", "id": id})
}

fn merged(id: &str, number: u64, at: &str, by: Value) -> Value {
    let mut row = item(id, number);
    row["__typename"] = json!("PullRequest");
    row["mergedAt"] = json!(at);
    row["mergedBy"] = by;
    row
}

fn closed(id: &str, at: &str, by: Value) -> Value {
    json!({"id": id, "createdAt": at, "actor": by})
}

fn close_candidate(id: &str, number: u64, closes: Value, is_pull: bool) -> Value {
    let mut row = item(id, number);
    let (kind, field) = if is_pull {
        ("PullRequest", "pullTimeline")
    } else {
        ("Issue", "issueTimeline")
    };
    row["__typename"] = json!(kind);
    row[field] = closes;
    row
}

fn default_query(query: &str) -> Reply {
    if query == commits::REPOSITORIES {
        return answer(commit_discovery(&[], 0));
    }
    standard_query(query).expect("unexpected query")
}

#[tokio::test]
async fn one_date_qualified_request_collects_only_the_viewers_merges_and_closes() {
    let server = TestServer::start(|request| {
        let body = request_body(request);
        match body["query"].as_str().unwrap() {
            OPENED_PULL_REQUESTS => answer(contributions(
                vec![json!({"occurredAt": START, "item": item("pr-a", 12)})],
                None,
            )),
            SEARCHES => {
                let range = format!("{START}..{END}");
                let vars = &body["variables"];
                assert_eq!(
                    vars["mergeInvolved"],
                    format!("is:pr is:merged involves:jdoe sort:created-asc merged:{range}")
                );
                assert_eq!(
                    vars["mergeReviewed"],
                    format!("is:pr is:merged reviewed-by:jdoe sort:created-asc merged:{range}")
                );
                assert_eq!(
                    vars["closePulls"],
                    format!(
                        "is:pr is:closed is:unmerged involves:jdoe sort:created-asc closed:{range}"
                    )
                );
                assert_eq!(
                    vars["closeIssues"],
                    format!("is:issue is:closed involves:jdoe sort:created-asc closed:{range}")
                );
                assert_eq!(vars["since"], START);
                let merger = merged("pr-a", 12, IN_RANGE, actor("U_example"));
                searches(
                    search_page(
                        3,
                        vec![
                            merger.clone(),
                            merged("pr-not-mine", 13, IN_RANGE, actor("U_other")),
                            merged("pr-too-late", 14, END, actor("U_example")),
                        ],
                        None,
                    ),
                    search_page(
                        2,
                        vec![
                            merger,
                            merged("pr-review-only", 15, IN_RANGE, actor("U_example")),
                        ],
                        None,
                    ),
                    search_page(
                        1,
                        vec![close_candidate(
                            "pr-closed",
                            16,
                            page(vec![closed("close-pr", IN_RANGE, actor("U_example"))], None),
                            true,
                        )],
                        None,
                    ),
                    search_page(
                        1,
                        vec![close_candidate(
                            "issue-a",
                            5,
                            page(
                                vec![
                                    closed("first-close", START, actor("U_example")),
                                    closed("other-close", IN_RANGE, actor("U_other")),
                                    closed("missing-actor", IN_RANGE, Value::Null),
                                    closed("second-close", IN_RANGE, actor("U_example")),
                                    closed("after-end", END, actor("U_example")),
                                ],
                                None,
                            ),
                            false,
                        )],
                        None,
                    ),
                )
            }
            query => default_query(query),
        }
    });

    let entries = collect(&api(&server), options()).await.unwrap();
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>(),
        [
            "pr-a",
            "pr-a:merged",
            "pr-review-only:merged",
            "close-pr",
            "first-close",
            "second-close"
        ],
    );
    assert!(
        matches!(&entries[1].action, GitHubAction::Merged { pull_request } if pull_request.number == 12)
    );
    assert!(
        matches!(&entries[3].action, GitHubAction::Closed { target: Item::PullRequest(pull) } if pull.number == 16)
    );
    assert!(
        matches!(&entries[5].action, GitHubAction::Closed { target: Item::Issue(issue) } if issue.number == 5)
    );
    assert_eq!(state_change_requests(&server).len(), 1);
}

fn state_change_requests(server: &TestServer) -> Vec<Value> {
    server
        .requests()
        .iter()
        .map(request_body)
        .filter(|body| {
            matches!(
                body["query"].as_str(),
                Some(SEARCHES | SEARCH | MORE_CLOSE_EVENTS)
            )
        })
        .collect()
}

// Live check on a GitHub token: confirm the date qualifiers select the same
// window and that a candidate with several closes returns each event and actor.
#[tokio::test]
async fn saturated_search_splits_and_uses_the_original_start_for_close_events() {
    const MIDDLE: &str = "2026-11-11T00:00:00Z";
    let server = TestServer::start(|request| {
        let body = request_body(request);
        let vars = &body["variables"];
        match body["query"].as_str().unwrap() {
            SEARCHES => {
                let empty = search_page(0, vec![], None);
                searches(
                    empty.clone(),
                    empty.clone(),
                    empty,
                    search_page(1_000, vec![], Some("cap")),
                )
            }
            SEARCH => {
                assert_eq!(vars["since"], START);
                match vars["query"]
                    .as_str()
                    .unwrap()
                    .split_whitespace()
                    .last()
                    .unwrap()
                {
                    q if q == format!("closed:{START}..{MIDDLE}") => {
                        assert!(vars["after"].is_null());
                        answer(json!({"closes": search_page(0, vec![], None)}))
                    }
                    q if q == format!("closed:{MIDDLE}..{END}") && vars["after"].is_null() => {
                        answer(json!({"closes": search_page(101, vec![], Some("more-matches"))}))
                    }
                    q if q == format!("closed:{MIDDLE}..{END}") => {
                        assert_eq!(vars["after"], "more-matches");
                        let closes = page(
                            vec![closed("early-close", START, actor("U_example"))],
                            Some("reclosed"),
                        );
                        answer(json!({"closes": search_page(101, vec![
                            close_candidate("issue-reopened", 5, closes, false)
                        ], None)}))
                    }
                    other => panic!("unexpected search {other}"),
                }
            }
            MORE_CLOSE_EVENTS => {
                assert_eq!(vars["id"], "issue-reopened");
                assert_eq!(vars["after"], "reclosed");
                assert_eq!(vars["since"], START);
                answer(json!({"node": {
                    "__typename": "Issue", "id": "issue-reopened",
                    "issueTimeline": page(vec![closed("later-close", IN_RANGE, actor("U_example"))], None)
                }}))
            }
            query => default_query(query),
        }
    });
    let entries = collect(&api(&server), options()).await.unwrap();
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>(),
        ["early-close", "later-close"]
    );
    assert_eq!(state_change_requests(&server).len(), 5);
}

#[tokio::test]
async fn close_timeline_stops_after_five_pages_and_keeps_the_events_read() {
    let server = TestServer::start(|request| {
        let body = request_body(request);
        match body["query"].as_str().unwrap() {
            SEARCHES => {
                let empty = search_page(0, vec![], None);
                searches(
                    empty.clone(),
                    empty.clone(),
                    empty,
                    search_page(
                        1,
                        vec![close_candidate(
                            "issue-a",
                            5,
                            page(
                                vec![closed("event-0", START, actor("U_example"))],
                                Some("1"),
                            ),
                            false,
                        )],
                        None,
                    ),
                )
            }
            MORE_CLOSE_EVENTS => {
                let after = body["variables"]["after"]
                    .as_str()
                    .unwrap()
                    .parse::<u64>()
                    .unwrap();
                let next = (after + 1).to_string();
                answer(json!({"node": {
                    "__typename": "Issue", "id": "issue-a",
                    "issueTimeline": page(vec![
                        closed(&format!("event-{after}"), IN_RANGE, actor("U_example"))
                    ], Some(&next))
                }}))
            }
            query => default_query(query),
        }
    });
    let entries = collect(&api(&server), options()).await.unwrap();
    assert_eq!(entries.len(), 5);
    assert_eq!(entries.last().unwrap().id, "event-4");
    assert_eq!(state_change_requests(&server).len(), 5);
}

#[tokio::test]
async fn repeating_a_close_event_page_is_a_typed_failure() {
    let server = TestServer::start(|request| {
        let body = request_body(request);
        let event = closed("event-0", START, actor("U_example"));
        match body["query"].as_str().unwrap() {
            SEARCHES => {
                let empty = search_page(0, vec![], None);
                searches(
                    empty.clone(),
                    empty.clone(),
                    empty,
                    search_page(
                        1,
                        vec![close_candidate(
                            "issue-a",
                            5,
                            page(vec![event], Some("next")),
                            false,
                        )],
                        None,
                    ),
                )
            }
            MORE_CLOSE_EVENTS => answer(json!({"node": {
                "__typename": "Issue", "id": "issue-a", "issueTimeline": page(vec![event], None)
            }})),
            query => default_query(query),
        }
    });
    let error = collect(&api(&server), options()).await.unwrap_err();
    assert_eq!(
        error.downcast_ref::<PageError>(),
        Some(&PageError::RepeatedPage)
    );
}

fn refusal(data: Value, kind: &str, path: Value) -> Reply {
    Reply::json(
        200,
        json!({
            "data": data,
            "errors": [{"type": kind, "path": path, "message": "Example response"}],
        })
        .to_string(),
    )
}

fn search_data(merge_involved: Value) -> Value {
    let empty = search_page(0, vec![], None);
    json!({"mergeInvolved":merge_involved, "mergeReviewed":empty, "closePulls":empty, "closeIssues":empty})
}

#[tokio::test]
async fn node_scoped_search_refusals_skip_candidates_and_warn_once_across_chunks() {
    const OLDER: &str = "2026-10-20T00:00:00Z";
    let server = TestServer::start(|request| {
        let body = request_body(request);
        let vars = &body["variables"];
        match body["query"].as_str().unwrap() {
            SEARCHES => {
                let date = vars["since"].as_str().unwrap();
                let id = format!("initial-{date}");
                refusal(
                    search_data(search_page(
                        2,
                        vec![merged(&id, 12, date, actor("U_example")), Value::Null],
                        Some("next"),
                    )),
                    "FORBIDDEN",
                    json!(["mergeInvolved", "nodes", 1]),
                )
            }
            SEARCH => {
                assert_eq!(vars["after"], "next");
                let date = vars["since"].as_str().unwrap();
                let id = format!("continued-{date}");
                refusal(
                    json!({"merges":search_page(2, vec![
                        Value::Null, merged(&id, 13, date, actor("U_example")),
                    ], None)}),
                    "FORBIDDEN",
                    json!(["merges", "nodes", 0]),
                )
            }
            query => default_query(query),
        }
    });
    let api = api(&server);
    let connection = ConnectionId::new(Source::Github, "search-saml");
    let scope = connection.to_string();
    {
        let state = GitHubCollection::default();
        let run = state.run(&connection);
        for range in [
            options(),
            CollectOptions {
                start: OLDER.parse().unwrap(),
                end: START.parse().unwrap(),
            },
        ] {
            let entries = super::super::collect(&api, range, &run).await.unwrap();
            assert_eq!(entries.len(), 2);
            assert!(entries[0].id.starts_with("initial-"));
            assert!(entries[1].id.starts_with("continued-"));
        }
    }
    assert_eq!(state_change_requests(&server).len(), 4);
    assert_eq!(
        warnings(&scope),
        ["Skipped 4 refused search results from GitHub"]
    );
}

#[tokio::test]
async fn alias_and_search_root_refusals_fail_with_their_typed_cause() {
    for path in [json!(["mergeInvolved"]), json!(["search"])] {
        let server = TestServer::start(move |request| {
            let body = request_body(request);
            match body["query"].as_str().unwrap() {
                SEARCHES => refusal(search_data(Value::Null), "FORBIDDEN", path.clone()),
                query => default_query(query),
            }
        });
        let remote = collect(&api(&server), options())
            .await
            .unwrap_err()
            .downcast::<RemoteFailure>()
            .unwrap();
        assert!(matches!(remote.error, ApiError::Query(refusal)
            if refusal.kinds().eq([QueryErrorKind::Forbidden])));
    }
}

#[tokio::test]
async fn a_forbidden_close_timeline_preserves_events_already_read() {
    let server = TestServer::start(|request| {
        let body = request_body(request);
        match body["query"].as_str().unwrap() {
            SEARCHES => {
                let empty = search_page(0, vec![], None);
                searches(
                    empty.clone(),
                    empty.clone(),
                    empty,
                    search_page(
                        1,
                        vec![close_candidate(
                            "issue-a",
                            5,
                            page(
                                vec![closed("first-close", START, actor("U_example"))],
                                Some("next"),
                            ),
                            false,
                        )],
                        None,
                    ),
                )
            }
            MORE_CLOSE_EVENTS => refusal(
                json!({"node":null}),
                "FORBIDDEN",
                json!(["node", "issueTimeline"]),
            ),
            query => default_query(query),
        }
    });
    let entries = collect(&api(&server), options()).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, "first-close");
}

#[tokio::test]
async fn a_busy_search_uses_the_shared_throttled_problem() {
    for graphql in [false, true] {
        let server = TestServer::start(move |request| {
            let body = request_body(request);
            match body["query"].as_str().unwrap() {
                SEARCHES if graphql => refusal(
                    search_data(search_page(1, vec![Value::Null], None)),
                    "RATE_LIMITED",
                    json!(["mergeInvolved", "nodes", 0]),
                ),
                SEARCHES => Reply::json(403, r#"{"message":"Example limit"}"#)
                    .with_header("X-RateLimit-Remaining", "0"),
                query => default_query(query),
            }
        });
        let remote = collect(&api(&server), options())
            .await
            .unwrap_err()
            .downcast::<RemoteFailure>()
            .unwrap();
        assert_eq!(remote.error.problem(), Some(ProviderProblem::Throttled));
        assert_eq!(state_change_requests(&server).len(), 1);
        if graphql {
            assert!(matches!(remote.error, ApiError::Query(refusal)
                if refusal.kinds().eq([QueryErrorKind::RateLimited])));
        } else {
            assert!(matches!(remote.error, ApiError::Status { .. }));
        }
    }
}

#[tokio::test]
async fn a_row_of_the_wrong_search_kind_fails_instead_of_being_skipped() {
    for bad in [
        item("malformed", 7),
        close_candidate("issue", 7, page(vec![], None), false),
    ] {
        let server = TestServer::start(move |request| {
            let body = request_body(request);
            match body["query"].as_str().unwrap() {
                SEARCHES => answer(search_data(search_page(1, vec![bad.clone()], None))),
                query => default_query(query),
            }
        });
        let error = collect(&api(&server), options()).await.unwrap_err();
        assert_eq!(
            error.downcast_ref::<CollectionError>(),
            Some(&CollectionError::StateChangeKindMismatch)
        );
    }
}

fn warnings(scope: &str) -> Vec<String> {
    diagnostics::entries()
        .into_iter()
        .filter(|line| line.scope == scope && line.level == LogLevel::Warn)
        .map(|line| line.message)
        .collect()
}

#[tokio::test]
async fn saturated_splits_reserve_first_pages_and_stop_at_the_search_request_budget() {
    let server = TestServer::start(|request| {
        let body = request_body(request);
        match body["query"].as_str().unwrap() {
            SEARCHES => answer(search_data(search_page(1_000, vec![], Some("next")))),
            SEARCH => {
                let after = body["variables"]["after"].is_null().then_some("next");
                answer(json!({"merges": search_page(1_000, vec![], after)}))
            }
            query => default_query(query),
        }
    });
    let start = START.parse().unwrap();
    let range = CollectOptions {
        start,
        end: start + chrono::TimeDelta::days(64),
    };
    let connection = ConnectionId::new(Source::Github, "saturated-splits");
    let scope = connection.to_string();
    let api = api(&server);
    diagnostics::in_connection(connection, collect(&api, range))
        .await
        .unwrap();

    let requests = state_change_requests(&server);
    assert_eq!(requests.len(), SEARCH_REQUESTS_PER_WINDOW);
    let last = &requests.last().unwrap()["variables"];
    assert!(last["after"].is_null());
    assert!(
        last["query"]
            .as_str()
            .unwrap()
            .ends_with("merged:2026-12-03T00:00:00Z..2027-01-04T00:00:00Z")
    );
    assert_eq!(
        warnings(&scope),
        ["GitHub's result limit was reached in merge or close searches (10 affected)."]
    );
}

#[tokio::test]
async fn search_page_cap_and_one_day_saturation_emit_one_warning_each() {
    for (count, scope_suffix, warning) in [
        (
            1000,
            "saturated-day",
            "GitHub's result limit was reached in merge or close searches (1 affected).",
        ),
        (
            12,
            "search-page-limit",
            "The request limit was reached in GitHub merge or close searches (1 affected).",
        ),
    ] {
        let server = TestServer::start(move |request| {
            let body = request_body(request);
            match body["query"].as_str().unwrap() {
                SEARCHES => answer(search_data(search_page(count, vec![], Some("1")))),
                SEARCH => {
                    let page_number: u32 = body["variables"]["after"]
                        .as_str()
                        .expect("a one-day window cannot split")
                        .parse()
                        .unwrap();
                    let next = (page_number + 1).to_string();
                    answer(json!({"merges": search_page(count, vec![], Some(&next))}))
                }
                query => default_query(query),
            }
        });
        let start = START.parse().unwrap();
        let range = CollectOptions {
            start,
            end: start + chrono::TimeDelta::days(1),
        };
        let connection = ConnectionId::new(Source::Github, scope_suffix);
        let scope = connection.to_string();
        let api = api(&server);
        diagnostics::in_connection(connection, collect(&api, range))
            .await
            .unwrap();

        assert_eq!(state_change_requests(&server).len(), SEARCH_PAGES);
        assert_eq!(warnings(&scope), [warning]);
    }
}

#[tokio::test]
async fn the_close_continuation_budget_is_shared_across_candidates() {
    let server = TestServer::start(|request| {
        let body = request_body(request);
        match body["query"].as_str().unwrap() {
            SEARCHES => {
                let rows: Vec<_> = (0..=CLOSE_REQUESTS_PER_WINDOW)
                    .map(|index| {
                        let id = format!("issue-{index}");
                        close_candidate(
                            &id,
                            index as u64 + 1,
                            page(
                                vec![closed(&format!("{id}-first"), START, actor("U_example"))],
                                Some("next"),
                            ),
                            false,
                        )
                    })
                    .collect();
                let empty = search_page(0, vec![], None);
                searches(
                    empty.clone(),
                    empty.clone(),
                    empty,
                    search_page(rows.len() as u64, rows, None),
                )
            }
            MORE_CLOSE_EVENTS => {
                let id = body["variables"]["id"].as_str().unwrap();
                answer(json!({"node": {
                    "__typename":"Issue", "id":id, "issueTimeline":page(vec![
                        closed(&format!("{id}-second"), IN_RANGE, actor("U_example")),
                    ], None)
                }}))
            }
            query => default_query(query),
        }
    });
    let connection = ConnectionId::new(Source::Github, "close-request-cap");
    let scope = connection.to_string();
    let api = api(&server);
    let entries = diagnostics::in_connection(connection, collect(&api, options()))
        .await
        .unwrap();

    let requests = state_change_requests(&server);
    let count = requests
        .iter()
        .filter(|body| body["query"] == MORE_CLOSE_EVENTS)
        .count();
    assert_eq!(count, CLOSE_REQUESTS_PER_WINDOW);
    assert_eq!(entries.len(), 2 * CLOSE_REQUESTS_PER_WINDOW + 1);
    assert_eq!(
        entries.last().unwrap().id,
        format!("issue-{CLOSE_REQUESTS_PER_WINDOW}-first")
    );
    assert_eq!(
        warnings(&scope),
        ["The request limit was reached while reading GitHub close timelines (1 affected)."]
    );
}
