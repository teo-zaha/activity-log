use super::*;
use crate::collect::github::{
    commit_repositories::REPOSITORIES_PER_REQUEST,
    releases::{MORE_RELEASES_OPERATION, RELEASE_CONTINUATION_REQUESTS_PER_WINDOW},
};
use serde_json::Map;

const BEFORE: &str = "2026-10-01T10:00:00Z";
const PUBLISHED_LATER: &str = "2026-11-15T10:00:00Z";

fn release(id: &str, created: &str, published: Option<&str>) -> Value {
    json!({
        "id": id, "tagName": "v1.2.0", "name": "Widgets",
        "createdAt": created, "publishedAt": published, "isDraft": false,
        "isPrerelease": false, "author": {"id": "U_example"},
    })
}

fn repository(id: &str, nodes: Vec<Value>, after: Option<&str>) -> Value {
    json!({"id": id, "nameWithOwner": "acme/widgets", "releases": page(nodes, after)})
}

pub(super) fn with_empty_release_pages(mut data: Value, variables: &Value) -> Value {
    let nodes: Vec<_> = variables["releaseIds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| repository(id.as_str().unwrap(), vec![], None))
        .collect();
    data["releaseNodes"] = json!(nodes);
    data
}

fn discovery(ids: &[String], variables: &Value) -> Reply {
    let mut range = options();
    let mut ids = ids;
    let asked = CollectOptions {
        start: variables["from"].as_str().unwrap().parse().unwrap(),
        end: variables["to"].as_str().unwrap().parse().unwrap(),
    };
    while (range.start, range.end) != (asked.start, asked.end) && ids.len() > 100 {
        let (left, right) = split_window(range).unwrap();
        let (first, last) = ids.split_at(ids.len() / 2);
        (range, ids) = if asked.start < right.start {
            (left, first)
        } else {
            (right, last)
        };
    }
    let listed: Vec<_> = ids.iter().take(100).map(String::as_str).collect();
    answer(commit_discovery(&listed, ids.len()))
}

fn release_server(
    ids: Vec<String>,
    releases: impl Fn(&str, &Value, &Request) -> Value + Send + Sync + 'static,
) -> TestServer {
    TestServer::start(move |request| {
        let body = request_body(request);
        let query = body["query"].as_str().unwrap();
        let variables = &body["variables"];
        if let Some(reply) = standard_query(query) {
            return reply;
        }
        match query {
            commits::REPOSITORIES => discovery(&ids, variables),
            commits::HISTORY => {
                assert_eq!(variables["pageSize"], 100);
                let mut response = releases(query, variables, request);
                if response["data"].get("nodes").is_none() {
                    let nodes: Vec<_> = variables["ids"].as_array().unwrap().iter()
                        .map(|id| json!({"id": id, "nameWithOwner": "acme/widgets", "defaultBranchRef": null}))
                        .collect();
                    response["data"]["nodes"] = json!(nodes);
                }
                Reply::json(200, response.to_string())
            }
            query if query.starts_with(MORE_RELEASES_OPERATION) => {
                assert_eq!(variables["pageSize"], 100);
                Reply::json(200, releases(query, variables, request).to_string())
            }
            _ => panic!("unexpected operation {query}"),
        }
    })
}

fn release_requests(server: &TestServer, query: &str) -> usize {
    server
        .requests()
        .iter()
        .filter(|request| {
            request_body(request)["query"]
                .as_str()
                .unwrap()
                .starts_with(query)
        })
        .count()
}

fn warnings(name: &str) -> Vec<String> {
    let scope = ConnectionId::new(Source::Github, name).to_string();
    diagnostics::entries()
        .into_iter()
        .filter(|row| row.scope == scope && row.level == LogLevel::Warn)
        .map(|row| row.message)
        .collect()
}

async fn collect_windows(
    server: &TestServer,
    name: &str,
    windows: &[CollectOptions],
) -> Vec<Vec<GitHubEntry>> {
    let id = ConnectionId::new(Source::Github, name);
    let state = GitHubCollection::default();
    let run = state.run(&id);
    let api = api(server);
    let mut entries = Vec::new();
    for window in windows {
        entries.push(
            diagnostics::in_connection(id.clone(), super::super::collect(&api, *window, &run))
                .await
                .unwrap(),
        );
    }
    entries
}

async fn collect_releases(server: &TestServer, name: &str) -> Vec<GitHubEntry> {
    collect_windows(server, name, &[options()])
        .await
        .pop()
        .unwrap()
}

#[tokio::test]
async fn releases_filter_by_publication_and_author_and_continue_by_creation() {
    let server = release_server(vec!["repo-a".into()], |query, variables, request| {
        if query == commits::HISTORY {
            assert_eq!(variables["releaseIds"], json!(["repo-a"]));
            assert_eq!(variables["readReleases"], true);
            let mut start = release("R_start", START, Some(START));
            let host = request.header("host").unwrap();
            start["url"] = json!(format!("http://{host}/acme/widgets/releases/tag/v1.2.0"));
            start["isPrerelease"] = json!(true);
            let mut draft = release("R_draft", IN_RANGE, Some(IN_RANGE));
            draft["isDraft"] = json!(true);
            let mut other = release("R_other", START, Some(IN_RANGE));
            other["author"]["id"] = json!("U_other");
            let mut unowned = release("R_no_author", START, Some(IN_RANGE));
            unowned["author"] = Value::Null;
            return json!({"data": {"releaseNodes": [repository("repo-a", vec![
                release("R_end", END, Some(END)), draft, start, other, unowned,
                release("R_unpublished", START, None),
            ], Some("release-next"))]}});
        }
        assert_eq!(variables["r0Id"], "repo-a");
        assert_eq!(variables["r0After"], "release-next");
        let mut later = release("R_old_draft", BEFORE, Some(IN_RANGE));
        later["name"] = json!(" ");
        later["url"] = json!("https://elsewhere.example.org/releases/v1.2.0");
        // The in-range publication is retained even when this page stops the scan.
        json!({"data": {"r0": repository("repo-a", vec![
            later,
            release("R_before", BEFORE, Some(BEFORE)),
        ], Some("too-old"))}})
    });
    let entries = collect_releases(&server, "release-filter").await;
    let [first, later] = entries.as_slice() else {
        panic!("expected two releases")
    };
    assert_eq!(
        (first.id.as_str(), first.timestamp.as_str()),
        ("R_start", START)
    );
    assert_eq!(
        (later.id.as_str(), later.timestamp.as_str()),
        ("R_old_draft", IN_RANGE)
    );
    assert_eq!(first.repository, "acme/widgets");
    let GitHubAction::Released { release } = &first.action else {
        panic!("expected release")
    };
    assert_eq!(release.tag, "v1.2.0");
    assert_eq!(release.name.as_deref(), Some("Widgets"));
    assert_eq!(
        release.web_url.as_deref(),
        Some(
            format!(
                "{}/acme/widgets/releases/tag/v1.2.0",
                server.url.trim_end_matches('/')
            )
            .as_str()
        )
    );
    assert!(release.prerelease);
    assert!(matches!(
        &later.action,
        GitHubAction::Released { release }
        if release.name.is_none() && release.web_url.is_none() && !release.prerelease
    ));
    assert_eq!(release_requests(&server, commits::REPOSITORIES), 1);
    assert_eq!(release_requests(&server, commits::HISTORY), 1);
    assert_eq!(release_requests(&server, MORE_RELEASES_OPERATION), 1);
    assert_eq!(
        warnings("release-filter"),
        ["Omitted 1 GitHub links to another origin, first: https://elsewhere.example.org"]
    );
}

#[tokio::test]
async fn commit_and_release_aliases_keep_each_others_readable_data() {
    let server = release_server(
        vec!["repo-a".into(), "repo-b".into(), "repo-hidden".into()],
        |query, _, _| {
            assert_eq!(query, commits::HISTORY);
            json!({
                "data": {
                    "nodes": [
                        history_repository("repo-a", page(vec![], None)),
                        null,
                        history_repository("repo-hidden", page(vec![commit_row("C_visible", START)], None)),
                    ],
                    "releaseNodes": [
                        repository("repo-a", vec![release("R_visible", START, Some(START)), Value::Null], None),
                        repository("repo-b", vec![release("R_independent", START, Some(START))], None),
                        null,
                    ],
                },
                "errors": [
                    {"type": "FORBIDDEN", "path": ["releaseNodes", 0, "releases", "nodes", 1, "author"]},
                    {"type": "FORBIDDEN", "path": ["releaseNodes", 0, "releases", "nodes", 1]},
                    {"type": "FORBIDDEN", "path": ["nodes", 1, "defaultBranchRef"]},
                    {"type": "NOT_FOUND", "path": ["releaseNodes", 2, "releases"]},
                ],
            })
        },
    );
    let entries = collect_releases(&server, "release-refusals").await;
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>(),
        ["C_visible", "R_visible", "R_independent"]
    );
    assert_eq!(
        warnings("release-refusals"),
        [
            "Could not read commit history for 1 GitHub repositories. Check repository access and Contents permission.",
            "Could not read all releases for 1 GitHub repositories. Check repository access and Contents permission.",
            "Skipped 1 GitHub releases the token could not read",
        ]
    );
}

#[tokio::test]
async fn a_null_release_alias_with_scoped_denial_keeps_the_commits() {
    let server = release_server(vec!["repo-a".into()], |query, _, _| {
        assert_eq!(query, commits::HISTORY);
        json!({
            "data": {
                "nodes": [history_repository("repo-a", page(vec![commit_row("C_visible", START)], None))],
                "releaseNodes": null,
            },
            "errors": [{"type": "FORBIDDEN", "path": ["releaseNodes", 0, "releases"]}],
        })
    });
    let entries = collect_releases(&server, "null-release-alias").await;
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>(),
        ["C_visible"]
    );
    assert_eq!(
        warnings("null-release-alias"),
        [
            "Could not read all releases for 1 GitHub repositories. Check repository access and Contents permission.",
        ]
    );
}

#[tokio::test]
async fn a_release_row_refusal_requires_a_readable_in_bounds_slot() {
    let server = release_server(vec!["repo-a".into()], |query, _, _| {
        assert_eq!(query, commits::HISTORY);
        json!({
            "data": {"releaseNodes": [repository("repo-a", vec![], None)]},
            "errors": [{"type": "FORBIDDEN", "path": ["releaseNodes", 0, "releases", "nodes", 1]}],
        })
    });
    let error = collect(&api(&server), options()).await.unwrap_err();
    assert!(matches!(
        error.downcast_ref::<RemoteFailure>().map(|failure| &failure.error),
        Some(ApiError::Query(refusal))
            if refusal.kinds().collect::<Vec<_>>() == [QueryErrorKind::Forbidden]
    ));
}

#[tokio::test]
async fn a_batched_release_continuation_skips_only_the_named_slot() {
    let server = release_server(
        vec!["repo-a".into(), "repo-b".into()],
        |query, variables, _| {
            if query == commits::HISTORY {
                return json!({"data": {"releaseNodes": [
                    repository("repo-a", vec![release("R_first", START, Some(START))], Some("a-next")),
                    repository("repo-b", vec![], None),
                ]}});
            }
            assert_eq!(variables["r0Id"], "repo-a");
            assert_eq!(variables["r0After"], "a-next");
            assert!(variables["r1Id"].is_null());
            json!({
                "data": {"r0": repository("repo-a", vec![
                    Value::Null, release("R_second", START, Some(START)),
                ], None)},
                "errors": [{"type": "FORBIDDEN", "path": ["r0", "releases", "nodes", 0]}],
            })
        },
    );
    let entries = collect_releases(&server, "release-continuation-refusal").await;
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>(),
        ["R_first", "R_second"]
    );
    assert_eq!(
        warnings("release-continuation-refusal"),
        ["Skipped 1 GitHub releases the token could not read"]
    );
}

#[tokio::test]
async fn a_continuation_for_another_repository_is_a_typed_error() {
    let server = release_server(vec!["repo-a".into()], |query, _, _| {
        if query == commits::HISTORY {
            json!({"data": {"releaseNodes": [repository(
                "repo-a", vec![release("R_first", START, Some(START))], Some("next")
            )]}})
        } else {
            json!({"data": {"r0": repository("repo-elsewhere", vec![], None)}})
        }
    });
    let error = collect(&api(&server), options()).await.unwrap_err();
    assert_eq!(
        error.downcast_ref::<CollectionError>(),
        Some(&CollectionError::ReleaseRepositoryMismatch)
    );
}

#[tokio::test]
async fn a_repo_refusal_on_a_later_page_keeps_previously_read_releases() {
    let server = release_server(vec!["repo-a".into()], |query, variables, _| {
        if query == commits::HISTORY {
            if variables["readReleases"] == false {
                return json!({"data": {}});
            }
            return json!({"data": {"releaseNodes": [repository(
                "repo-a", vec![release("R_first", START, Some(START))], Some("next")
            )]}});
        }
        json!({
            "data": {"r0": null},
            "errors": [{"type": "FORBIDDEN", "path": ["r0", "releases"]}],
        })
    });
    let rounds = collect_windows(&server, "release-later-refusal", &[options(), options()]).await;
    for entries in rounds {
        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.id.as_str())
                .collect::<Vec<_>>(),
            ["R_first"]
        );
    }
    assert_eq!(release_requests(&server, MORE_RELEASES_OPERATION), 1);
    assert_eq!(
        warnings("release-later-refusal"),
        [
            "Could not read all releases for 1 GitHub repositories. Check repository access and Contents permission.",
        ]
    );
}

#[tokio::test]
async fn a_release_page_with_every_row_unreadable_is_counted_as_cut_off() {
    for (refusal, scope, expected) in [
        (
            true,
            "refused-release-page",
            [
                "Stopped reading GitHub releases early in 1 repositories",
                "Skipped 1 GitHub releases the token could not read",
            ],
        ),
        (
            false,
            "malformed-release-page",
            [
                "Skipped 1 GitHub rows that could not be read",
                "Stopped reading GitHub releases early in 1 repositories",
            ],
        ),
    ] {
        let server = release_server(vec!["repo-a".into()], move |query, _, _| {
            assert_eq!(query, commits::HISTORY);
            let mut response = json!({
                "data": {"releaseNodes": [repository("repo-a", vec![Value::Null], Some("next"))]},
            });
            if refusal {
                response["errors"] = json!([
                    {"type": "FORBIDDEN", "path": ["releaseNodes", 0, "releases", "nodes", 0]},
                ]);
            }
            response
        });
        assert!(collect_releases(&server, scope).await.is_empty());
        assert_eq!(release_requests(&server, MORE_RELEASES_OPERATION), 0);
        assert_eq!(warnings(scope), expected);
    }
}

#[tokio::test]
async fn an_older_disjoint_chunk_resumes_the_release_cursor() {
    let server = release_server(vec!["repo-a".into()], |query, variables, _| {
        if query == commits::HISTORY {
            if variables["from"] == IN_RANGE {
                assert_eq!(variables["readReleases"], true);
                assert_eq!(variables["releaseIds"], json!(["repo-a"]));
                return json!({"data": {"releaseNodes": [repository(
                    "repo-a", vec![release("R_newer", START, Some(PUBLISHED_LATER))], Some("older")
                )]}});
            }
            assert_eq!(variables["from"], START);
            assert_eq!(variables["to"], IN_RANGE);
            assert_eq!(variables["readReleases"], false);
            assert_eq!(variables["releaseIds"], json!([]));
            return json!({"data": {}});
        }
        assert_eq!(variables["r0Id"], "repo-a");
        assert_eq!(variables["r0After"], "older");
        json!({"data": {"r0": repository(
            "repo-a", vec![release("R_older", BEFORE, Some(START))], None
        )}})
    });
    let boundary = IN_RANGE.parse().unwrap();
    let newer = CollectOptions {
        start: boundary,
        ..options()
    };
    let older = CollectOptions {
        end: boundary,
        ..options()
    };
    let rounds = collect_windows(&server, "release-disjoint-chunks", &[newer, older]).await;
    let ids: Vec<Vec<_>> = rounds
        .iter()
        .map(|entries| entries.iter().map(|entry| entry.id.as_str()).collect())
        .collect();
    assert_eq!(ids, [vec!["R_newer"], vec!["R_older"]]);
    assert_eq!(release_requests(&server, commits::HISTORY), 2);
    assert_eq!(release_requests(&server, MORE_RELEASES_OPERATION), 1);
}

#[tokio::test]
async fn reused_release_pages_keep_each_windows_own_cutoff() {
    let server = release_server(vec!["repo-a".into()], |query, variables, _| {
        if query == commits::HISTORY {
            if variables["readReleases"] == true {
                return json!({"data": {"releaseNodes": [repository(
                    "repo-a", vec![release("R_first", IN_RANGE, Some(PUBLISHED_LATER))], Some("older")
                )]}});
            }
            assert_eq!(variables["releaseIds"], json!([]));
            return json!({"data": {}});
        }
        assert_eq!(variables["r0After"], "older");
        json!({"data": {"r0": repository(
            "repo-a", vec![release("R_old_draft", BEFORE, Some(PUBLISHED_LATER))], None
        )}})
    });
    let narrow = CollectOptions {
        start: PUBLISHED_LATER.parse().unwrap(),
        ..options()
    };
    let rounds =
        collect_windows(&server, "release-memo-cutoff", &[narrow, options(), narrow]).await;
    let ids: Vec<Vec<_>> = rounds
        .iter()
        .map(|entries| entries.iter().map(|entry| entry.id.as_str()).collect())
        .collect();
    assert_eq!(
        ids,
        [
            vec!["R_first"],
            vec!["R_first", "R_old_draft"],
            vec!["R_first"]
        ]
    );
    let first_pages = server
        .requests()
        .iter()
        .filter(|request| {
            let body = request_body(request);
            body["query"] == commits::HISTORY && body["variables"]["readReleases"] == true
        })
        .count();
    assert_eq!(first_pages, 1);
    assert_eq!(release_requests(&server, MORE_RELEASES_OPERATION), 1);
}

#[tokio::test]
async fn release_page_cap_warns_once_and_reuses_pages_across_chunks() {
    let server = release_server(vec!["repo-a".into()], |query, variables, _| {
        if query == commits::HISTORY {
            if variables["readReleases"] == false {
                return json!({"data": {}});
            }
            return json!({"data": {"releaseNodes": [repository(
                "repo-a", vec![release("R_one", START, Some(START))], Some("2")
            )]}});
        }
        let after = variables["r0After"].as_str().unwrap();
        let (id, next) = match after {
            "2" => ("R_two", "3"),
            "3" => ("R_three", "4"),
            _ => panic!("unexpected release cursor {after}"),
        };
        json!({"data": {"r0": repository("repo-a", vec![release(id, START, Some(START))], Some(next))}})
    });
    let rounds = collect_windows(&server, "release-page-cap", &[options(), options()]).await;
    assert_eq!(rounds.iter().map(Vec::len).collect::<Vec<_>>(), [3, 3]);
    assert_eq!(release_requests(&server, MORE_RELEASES_OPERATION), 2);
    assert_eq!(
        warnings("release-page-cap"),
        ["Stopped reading GitHub releases early in 1 repositories"]
    );
}

#[tokio::test]
async fn release_continuations_are_batched_and_stop_at_the_window_allowance() {
    let count = RELEASE_CONTINUATION_REQUESTS_PER_WINDOW * REPOSITORIES_PER_REQUEST / 2 + 1;
    let last = format!("repo-{:03}", count - 1);
    let ids = (0..count).map(|n| format!("repo-{n:03}")).collect();
    let server = release_server(ids, move |query, variables, _| {
        if query == commits::HISTORY {
            let ids = variables["releaseIds"].as_array().unwrap();
            assert!(ids.len() <= REPOSITORIES_PER_REQUEST);
            let nodes: Vec<_> = ids
                .iter()
                .map(|id| {
                    let id = id.as_str().unwrap();
                    repository(
                        id,
                        vec![release(&format!("R_one_{id}"), START, Some(START))],
                        Some("next"),
                    )
                })
                .collect();
            return json!({"data": {"releaseNodes": nodes}});
        }
        let mut nodes = Map::new();
        for index in 0..REPOSITORIES_PER_REQUEST {
            let id = variables[format!("r{index}Id")].as_str().unwrap();
            let cursor = variables[format!("r{index}After")].as_str().unwrap();
            assert_ne!(
                id, last,
                "the last repository exceeds the window's allowance"
            );
            let (page, next) = match cursor {
                "next" => ("two", Some("last")),
                "last" => ("three", None),
                _ => panic!("unexpected cursor {cursor}"),
            };
            nodes.insert(
                format!("r{index}"),
                repository(
                    id,
                    vec![release(&format!("R_{page}_{id}"), START, Some(START))],
                    next,
                ),
            );
        }
        assert!(variables["r20Id"].is_null());
        json!({"data": nodes})
    });
    let entries = collect_releases(&server, "release-request-cap").await;
    assert_eq!(entries.len(), (count - 1) * 3 + 1);
    assert_eq!(
        release_requests(&server, MORE_RELEASES_OPERATION),
        RELEASE_CONTINUATION_REQUESTS_PER_WINDOW
    );
    assert_eq!(
        warnings("release-request-cap"),
        ["Stopped reading GitHub releases early in 1 repositories"]
    );
}

#[tokio::test]
async fn repeated_release_cursors_fail_before_another_request() {
    let server = release_server(vec!["repo-a".into()], |query, _, _| {
        if query == commits::HISTORY {
            json!({"data": {"releaseNodes": [repository("repo-a", vec![
                release("R_one", START, Some(START)),
            ], Some("repeat"))]}})
        } else {
            json!({"data": {"r0": repository("repo-a", vec![
                release("R_two", START, Some(START)),
            ], Some("repeat"))}})
        }
    });
    let error = collect(&api(&server), options()).await.unwrap_err();
    assert_eq!(
        error.downcast_ref::<PageError>(),
        Some(&PageError::RepeatedPage)
    );
    assert_eq!(release_requests(&server, MORE_RELEASES_OPERATION), 1);
}
