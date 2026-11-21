use std::{
    collections::BTreeMap,
    sync::{Arc, OnceLock},
};

use reqwest::StatusCode;
use serde_json::{Value, json};
use tokio::time::Instant;

use super::{GitLabChanges, kind_of};
use crate::{
    collect::change_stack::{
        Budget, ChangeLookup, ChangeRef, FailureKind, LookupCause, LookupFailure, OpenPage,
        change_stacks,
    },
    model::{
        ActivityEntry, ChangeStackAbove, ChangeStackBottom, ChangeStackEnd, CollectOptions,
        ConnectionId, ConnectionName, GitLabEntry, GitLabTag, GitLabType, LogLevel, Source,
        StackedChange, SystemEntry, SystemEntryType, SystemTag,
    },
    provider::{
        ProviderRuntime,
        connection::{Connection, ConnectionLocation, ConnectionValues},
        gitlab::{GitLabConfig, GitLabProvider, api::GitLabApi},
        remote_api::test_server::{Reply, Request, TestServer},
        remote_error::{ApiError, OAuthRefusal},
    },
};

const TOKEN: &str = "glpat-example-token";
/// `acme/widgets`, and its fork `dev/widgets`.
const WIDGETS: u64 = 42;
const FORK: u64 = 77;

fn api(url: &str) -> GitLabApi {
    GitLabApi::new(&GitLabConfig {
        url: url.to_string(),
        token: TOKEN.to_string(),
        user: "jdoe".to_string(),
    })
    .unwrap()
}

fn budget() -> Budget {
    Budget::new(50, Instant::now() + std::time::Duration::from_secs(20))
}

/// An open merge request of `acme/widgets` from `source` into `target`, linked on `base`.
fn mr(base: &str, iid: u64, source: &str, target: &str) -> Value {
    json!({
        "id": 1000 + iid,
        "iid": iid,
        "title": format!("Parse step {iid}"),
        "state": "opened",
        "source_branch": source,
        "target_branch": target,
        "source_project_id": WIDGETS,
        "target_project_id": WIDGETS,
        "web_url": format!("{base}/acme/widgets/-/merge_requests/{iid}"),
        "author": { "username": "jdoe" },
        "labels": ["backend"],
    })
}

/// Every value of `name` in the request's query, decoded.
fn values(request: &Request, name: &str) -> Vec<String> {
    let url = reqwest::Url::parse(&format!("http://local{}", request.target)).unwrap();
    url.query_pairs()
        .filter(|(key, _)| key == name)
        .map(|(_, value)| value.into_owned())
        .collect()
}

/// A GitLab that lists each project's rows as the API does: filtered by `iids[]`,
/// `source_branch` and `target_branch`, `page_size` a page, the next page named in
/// `x-next-page`. Rows are made once the server's address is known. Its token belongs to
/// `jdoe`, whose events are [`events`].
fn gitlab(
    page_size: usize,
    rows: impl Fn(&str) -> BTreeMap<u64, Vec<Value>> + Send + 'static,
) -> TestServer {
    let base = Arc::new(OnceLock::<String>::new());
    let known = base.clone();
    let server = TestServer::start(move |request| {
        let rows = rows(known.get().unwrap());
        let path = request.path();
        let segments: Vec<&str> = path
            .split_once("/api/v4/")
            .map(|(_, rest)| rest.split('/').collect())
            .unwrap_or_default();
        let project = match segments.as_slice() {
            ["user"] => return Reply::json(200, r#"{"id":7,"username":"jdoe"}"#),
            ["users", "jdoe", "events"] => return Reply::json(200, events().to_string()),
            ["projects", project, "merge_requests"] => *project,
            _ => return Reply::json(404, r#"{"message":"404 Not Found"}"#),
        };
        let Some(listed) = project.parse().ok().and_then(|id: u64| rows.get(&id)) else {
            return Reply::json(404, r#"{"message":"404 Project Not Found"}"#);
        };
        let iids = values(request, "iids[]");
        let source = request.query("source_branch");
        let target = request.query("target_branch");
        let matching: Vec<&Value> = listed
            .iter()
            .filter(|row| iids.is_empty() || iids.contains(&row["iid"].to_string()))
            .filter(|row| {
                source
                    .as_ref()
                    .is_none_or(|branch| row["source_branch"] == *branch)
            })
            .filter(|row| {
                target
                    .as_ref()
                    .is_none_or(|branch| row["target_branch"] == *branch)
            })
            .collect();
        let page: usize = request.query("page").unwrap().parse().unwrap();
        let shown: Vec<&&Value> = matching
            .iter()
            .skip((page - 1) * page_size)
            .take(page_size)
            .collect();
        let next = if matching.len() > page * page_size {
            (page + 1).to_string()
        } else {
            String::new()
        };
        Reply::json(200, json!(shown).to_string()).with_header("x-next-page", &next)
    });
    base.set(server.url.clone()).unwrap();
    server
}

/// `acme/widgets` with a stack of four: `main` ← !1 ← !2 ← !3 ← !4.
fn four(base: &str) -> BTreeMap<u64, Vec<Value>> {
    BTreeMap::from([(
        WIDGETS,
        vec![
            mr(base, 1, "feature/parse-a", "main"),
            mr(base, 2, "feature/parse-b", "feature/parse-a"),
            mr(base, 3, "feature/parse-c", "feature/parse-b"),
            mr(base, 4, "feature/parse-d", "feature/parse-c"),
        ],
    )])
}

fn numbers(changes: &[crate::collect::change_stack::OpenChange]) -> Vec<u64> {
    changes.iter().map(|change| change.number()).collect()
}

/// Every list asks for open merge requests of every author, 100 a page, with the token.
#[tokio::test]
async fn every_list_asks_for_every_open_merge_request_with_the_token() {
    let server = gitlab(100, four);
    let api = api(&format!("{}/gitlab/", server.url));
    let lookup = GitLabChanges::new(&api);
    let budget = budget();

    lookup.open_page(&WIDGETS, &budget).await.unwrap();
    lookup.open_numbers(&WIDGETS, &[2], &budget).await.unwrap();
    lookup
        .open_from(&WIDGETS, "feature/parse-a", &budget)
        .await
        .unwrap();
    lookup
        .open_into(&WIDGETS, "feature/parse-a", &budget)
        .await
        .unwrap();

    let requests = server.requests();
    assert_eq!(requests.len(), 4);
    for request in &requests {
        assert_eq!(request.path(), "/gitlab/api/v4/projects/42/merge_requests");
        assert_eq!(request.query("state").as_deref(), Some("opened"));
        assert_eq!(request.query("scope").as_deref(), Some("all"));
        assert_eq!(request.query("per_page").as_deref(), Some("100"));
        assert_eq!(request.header("private-token"), Some(TOKEN));
    }
    assert_eq!(values(&requests[1], "iids[]"), ["2"]);
    assert_eq!(
        requests[2].query("source_branch").as_deref(),
        Some("feature/parse-a")
    );
    assert_eq!(
        requests[3].query("target_branch").as_deref(),
        Some("feature/parse-a")
    );
    assert_eq!(budget.spent(), 4);
}

/// A page that names no next page holds every open merge request; one that names one holds
/// only some.
#[tokio::test]
async fn the_first_page_tells_whether_it_holds_them_all() {
    let whole = gitlab(100, four);
    let api_whole = api(&whole.url);
    let page = GitLabChanges::new(&api_whole)
        .open_page(&WIDGETS, &budget())
        .await
        .unwrap();
    assert!(matches!(&page, OpenPage::Complete(changes) if numbers(changes) == [1, 2, 3, 4]));

    let paged = gitlab(3, four);
    let api_paged = api(&paged.url);
    let budget = budget();
    let page = GitLabChanges::new(&api_paged)
        .open_page(&WIDGETS, &budget)
        .await
        .unwrap();
    assert!(matches!(&page, OpenPage::Partial(changes) if numbers(changes) == [1, 2, 3]));
    // Only the first page is read.
    assert_eq!(budget.spent(), 1);
    assert_eq!(paged.requests().len(), 1);
}

/// Numbers go 50 to a request, each listed by its own `iids[]`.
#[tokio::test]
async fn numbers_are_asked_for_fifty_at_a_time() {
    let server = gitlab(100, |base| {
        BTreeMap::from([(
            WIDGETS,
            (1..=120)
                .map(|iid| mr(base, iid, &format!("feature/{iid}"), "main"))
                .collect(),
        )])
    });
    let api = api(&server.url);
    let wanted: Vec<u64> = (1..=120).collect();

    let open = GitLabChanges::new(&api)
        .open_numbers(&WIDGETS, &wanted, &budget())
        .await
        .unwrap();

    assert_eq!(numbers(&open), wanted);
    let chunks: Vec<Vec<String>> = server
        .requests()
        .iter()
        .map(|request| values(request, "iids[]"))
        .collect();
    assert_eq!(
        chunks.iter().map(Vec::len).collect::<Vec<_>>(),
        [50, 50, 20]
    );
    assert_eq!(chunks[1][0], "51");
}

/// A branch reaches GitLab as itself, whatever it holds.
#[tokio::test]
async fn branch_names_are_sent_as_they_are() {
    let branch = "feature/a#b&c+d e?f=g";
    let server = gitlab(100, move |base| {
        BTreeMap::from([(WIDGETS, vec![mr(base, 7, branch, "main")])])
    });
    let api = api(&server.url);

    let found = GitLabChanges::new(&api)
        .open_from(&WIDGETS, branch, &budget())
        .await
        .unwrap();

    assert_eq!(numbers(&found), [7]);
    let request = &server.requests()[0];
    assert_eq!(request.query("source_branch").as_deref(), Some(branch));
    assert_eq!(request.query("state").as_deref(), Some("opened"));
    assert!(!request.target.contains(' '), "{}", request.target);
}

/// A list that brings `row` for a question about `!2`'s numbers or `feature/parse-a`.
fn answering(row: Value) -> TestServer {
    TestServer::start(move |_| Reply::json(200, json!([row]).to_string()))
}

/// A row the request did not ask for (another project, number or branch) means the server
/// ignored a filter: the list fails, and no further page is read.
#[tokio::test]
async fn a_row_that_was_not_asked_for_fails_the_list() {
    let base = "https://gitlab.example.org";
    let mut elsewhere = mr(base, 2, "feature/parse-b", "feature/parse-a");
    elsewhere["target_project_id"] = json!(FORK);
    let unasked = mr(base, 3, "feature/parse-b", "feature/parse-a");
    let other_branch = mr(base, 2, "feature/parse-x", "feature/parse-y");

    // Whether each question fails: its numbers (`!2`), its source branch (`feature/parse-b`)
    // and its target branch (`feature/parse-a`).
    for (name, row, fails) in [
        ("elsewhere", elsewhere, [true, true, true]),
        ("unasked", unasked, [true, false, false]),
        ("other branch", other_branch, [false, true, true]),
    ] {
        let server = answering(row);
        let api = api(&server.url);
        let lookup = GitLabChanges::new(&api);
        let budget = budget();
        let answers = [
            lookup.open_numbers(&WIDGETS, &[2], &budget).await.err(),
            lookup
                .open_from(&WIDGETS, "feature/parse-b", &budget)
                .await
                .err(),
            lookup
                .open_into(&WIDGETS, "feature/parse-a", &budget)
                .await
                .err(),
        ];
        let ignored = LookupFailure::caused_by(FailureKind::Failed, LookupCause::IgnoredFilter);
        let failed = answers.map(|answer| answer.is_some_and(|failure| failure == ignored));
        assert_eq!(failed, fails, "{name}");
    }
}

/// A row asked for that is no longer open (being merged, `locked`, or merged since) is left out
/// and counted; the rest of the list stands.
#[tokio::test]
async fn a_row_no_longer_open_is_left_out() {
    let server = gitlab(100, |base| {
        let mut locked = mr(base, 2, "feature/parse-b", "feature/parse-a");
        locked["state"] = json!("locked");
        let mut merged = mr(base, 3, "feature/parse-c", "feature/parse-a");
        merged["state"] = json!("merged");
        BTreeMap::from([(
            WIDGETS,
            vec![
                mr(base, 4, "feature/parse-d", "feature/parse-a"),
                locked,
                merged,
            ],
        )])
    });
    let api = api(&server.url);
    let lookup = GitLabChanges::new(&api);
    let budget = budget();

    let into = lookup
        .open_into(&WIDGETS, "feature/parse-a", &budget)
        .await
        .unwrap();
    let numbers_open = lookup
        .open_numbers(&WIDGETS, &[2, 3, 4], &budget)
        .await
        .unwrap();

    assert_eq!(numbers(&into), [4]);
    assert_eq!(numbers(&numbers_open), [4]);
    assert_eq!(lookup.not_open(), 4);
    assert_eq!(lookup.skipped(), 0);
}

/// A row that is no merge request, or one with a blank title or branch, is skipped and
/// counted; one from a deleted fork is kept, from no repository of its own.
#[tokio::test]
async fn odd_rows_are_skipped_and_counted() {
    let server = gitlab(100, |base| {
        let mut blank = mr(base, 2, " ", "main");
        blank["id"] = json!(2002);
        let mut untitled = mr(base, 3, "feature/parse-c", "main");
        untitled["title"] = json!("");
        let mut deleted_fork = mr(base, 4, "feature/parse-d", "main");
        deleted_fork["source_project_id"] = Value::Null;
        let mut from_fork = mr(base, 5, "feature/parse-e", "main");
        from_fork["source_project_id"] = json!(FORK);
        BTreeMap::from([(
            WIDGETS,
            vec![
                mr(base, 1, "feature/parse-a", "main"),
                blank,
                untitled,
                json!({ "id": 9, "iid": "nine" }),
                deleted_fork,
                from_fork,
            ],
        )])
    });
    let api = api(&server.url);
    let lookup = GitLabChanges::new(&api);

    let page = lookup.open_page(&WIDGETS, &budget()).await.unwrap();

    let OpenPage::Complete(changes) = page else {
        panic!("one page holds them all: {page:?}");
    };
    assert_eq!(numbers(&changes), [1, 4, 5]);
    let same_repo: Vec<bool> = changes.iter().map(|change| change.same_repo()).collect();
    assert_eq!(same_repo, [true, false, false]);
    assert_eq!(lookup.skipped(), 3);
}

/// A link is kept only when it leads to this GitLab's own pages, without user info.
#[tokio::test]
async fn links_lead_only_to_this_gitlab() {
    let server = gitlab(100, |base| {
        let mut elsewhere = mr(base, 2, "feature/parse-b", "main");
        elsewhere["web_url"] = json!("https://gitlab.example.org/acme/widgets/-/merge_requests/2");
        let mut with_user = mr(base, 3, "feature/parse-c", "main");
        with_user["web_url"] = json!(
            base.replace("http://", "http://jdoe:secret@") + "/acme/widgets/-/merge_requests/3"
        );
        let mut script = mr(base, 4, "feature/parse-d", "main");
        script["web_url"] = json!("javascript:alert(1)");
        let mut none = mr(base, 5, "feature/parse-e", "main");
        none["web_url"] = Value::Null;
        BTreeMap::from([(
            WIDGETS,
            vec![
                mr(base, 1, "feature/parse-a", "main"),
                elsewhere,
                with_user,
                script,
                none,
            ],
        )])
    });
    let api = api(&format!("{}/gitlab", server.url));
    let lookup = GitLabChanges::new(&api);

    let found = lookup.open_into(&WIDGETS, "main", &budget()).await.unwrap();

    let links: Vec<Option<String>> = found
        .iter()
        .map(|change| change.stacked().web_url)
        .collect();
    assert_eq!(
        links,
        [
            Some(format!("{}/acme/widgets/-/merge_requests/1", server.url)),
            None,
            None,
            None,
            None,
        ]
    );
    // Only the web page of another origin is counted, by that origin.
    assert_eq!(
        lookup.elsewhere(),
        (
            1,
            Some(
                reqwest::Url::parse("https://gitlab.example.org")
                    .unwrap()
                    .origin()
            )
        )
    );
}

/// Links dropped for leading to another origin are one warning per collection, which names
/// that origin and how many.
#[tokio::test]
async fn links_to_another_origin_are_one_warning() {
    let server = gitlab(100, |base| {
        let mut rows = four(base);
        for row in rows.get_mut(&WIDGETS).unwrap() {
            let iid = row["iid"].clone();
            row["web_url"] = json!(format!(
                "https://gitlab.internal:8443/acme/widgets/-/merge_requests/{iid}"
            ));
        }
        rows
    });
    let runtime = runtime(&server, "stack-elsewhere").await;
    let mut entries = vec![event("opened", GitLabType::Opened, Some(WIDGETS), Some(2))];

    runtime.annotate(&mut entries).await;

    assert_eq!(stacks(&entries)["opened"], Some(vec![1, 2]));
    let warnings: Vec<String> = lines("gitlab/stack-elsewhere")
        .into_iter()
        .filter(|line| line.level == LogLevel::Warn)
        .map(|line| line.message)
        .collect();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    // The origin is the data the line carries.
    assert!(
        warnings[0].contains("https://gitlab.internal:8443"),
        "{warnings:?}"
    );
}

/// The token goes to the configured origin only: a next page named elsewhere is not asked for.
#[tokio::test]
async fn a_next_page_elsewhere_is_never_asked_for() {
    let elsewhere = TestServer::start(|_| Reply::json(200, "[]"));
    let next = format!("{}/api/v4/projects/42/merge_requests?page=2", elsewhere.url);
    let server = TestServer::start(move |_| {
        Reply::json(
            200,
            json!([mr("https://gitlab.example.org", 1, "a", "main")]).to_string(),
        )
        .with_header("link", &format!("<{next}>; rel=\"next\""))
    });
    let api = api(&server.url);

    let answer = GitLabChanges::new(&api)
        .open_into(&WIDGETS, "main", &budget())
        .await;

    assert_eq!(
        answer.unwrap_err().kind(),
        FailureKind::Failed,
        "the page elsewhere is unreachable for this client"
    );
    assert!(elsewhere.requests().is_empty());
}

/// A list that comes back to a page it sent fails, as the events do.
#[tokio::test]
async fn a_repeated_page_fails_the_list() {
    let server = TestServer::start(|_| {
        Reply::json(
            200,
            json!([mr("https://gitlab.example.org", 1, "a", "main")]).to_string(),
        )
        .with_header("x-next-page", "1")
    });
    let api = api(&server.url);

    let answer = GitLabChanges::new(&api)
        .open_into(&WIDGETS, "main", &budget())
        .await;

    assert_eq!(
        answer.unwrap_err(),
        LookupFailure::caused_by(FailureKind::Failed, LookupCause::RepeatedPage)
    );
}

/// Each page is paid for before it is sent: a list longer than the budget stops at `Limit`.
#[tokio::test]
async fn each_page_is_paid_for_before_it_is_sent() {
    let server = gitlab(1, four);
    let api = api(&server.url);
    let budget = Budget::new(2, Instant::now() + std::time::Duration::from_secs(20));

    let answer = GitLabChanges::new(&api)
        .open_numbers(&WIDGETS, &[1, 2, 3], &budget)
        .await;

    assert_eq!(answer.unwrap_err().kind(), FailureKind::Limit);
    assert_eq!(server.requests().len(), 2);
}

fn status(status: u16, refusal: Option<OAuthRefusal>, retry_after: Option<u64>) -> ApiError {
    ApiError::Status {
        status: StatusCode::from_u16(status).unwrap(),
        message: String::new(),
        retry_after,
        refusal,
    }
}

/// The OAuth code decides first, then the status.
#[test]
fn failures_are_told_apart_by_code_then_status() {
    use FailureKind::*;
    use OAuthRefusal::*;
    for (error, kind) in [
        (status(401, Some(InvalidToken), None), TokenInvalid),
        (status(403, Some(InvalidToken), None), TokenInvalid),
        (status(401, Some(InsufficientScope), None), ScopeMissing),
        (status(403, Some(InsufficientScope), None), ScopeMissing),
        (status(400, Some(InvalidRequest), None), Failed),
        (status(403, Some(InvalidRequest), None), Failed),
        (status(401, None, None), TokenInvalid),
        (status(403, None, None), Unreadable),
        (status(404, None, None), Unreadable),
        (status(429, None, None), Throttled),
        (status(429, None, Some(60)), Throttled),
        (status(503, None, Some(5)), Throttled),
        (status(503, None, None), Failed),
        (status(500, None, None), Failed),
        (status(302, None, None), Failed),
        (ApiError::Timeout, Failed),
        (ApiError::Unreachable, Failed),
        (ApiError::Decode, Failed),
    ] {
        assert_eq!(kind_of(&error), kind, "{error:?}");
    }
}

/// What GitLab answers is classified as it sends it, header or body, and the request's own
/// failure is kept as the cause.
#[tokio::test]
async fn refusals_are_read_from_the_answer() {
    let scope = r#"Bearer realm="", error="insufficient_scope", error_description="x""#;
    for (reply, kind) in [
        (
            Reply::json(403, r#"{"error":"insufficient_scope"}"#),
            FailureKind::ScopeMissing,
        ),
        (
            Reply::json(401, "{}").with_header("www-authenticate", scope),
            FailureKind::ScopeMissing,
        ),
        (
            Reply::json(401, r#"{"error":"invalid_token"}"#),
            FailureKind::TokenInvalid,
        ),
        (
            Reply::json(401, r#"{"message":"401 Unauthorized"}"#),
            FailureKind::TokenInvalid,
        ),
        (
            Reply::json(403, r#"{"message":"403 Forbidden"}"#),
            FailureKind::Unreadable,
        ),
        (
            Reply::json(404, r#"{"message":"404 Project Not Found"}"#),
            FailureKind::Unreadable,
        ),
        (
            Reply::json(500, r#"{"message":"500 Internal Server Error"}"#),
            FailureKind::Failed,
        ),
        (
            Reply::html(200, "<html>sign in</html>"),
            FailureKind::Failed,
        ),
    ] {
        let reply = std::sync::Mutex::new(Some(reply));
        let server = TestServer::start(move |_| reply.lock().unwrap().take().unwrap());
        let api = api(&server.url);

        let failure = GitLabChanges::new(&api)
            .open_page(&WIDGETS, &budget())
            .await
            .unwrap_err();

        assert_eq!(failure.kind(), kind, "{failure:?}");
        assert!(
            matches!(failure.cause(), Some(LookupCause::Request(_))),
            "{failure:?}"
        );
    }
}

/// A busy answer is not retried, though it asks for a short wait: one request, `Throttled`.
#[tokio::test]
async fn a_busy_host_is_asked_once() {
    let replies: [fn() -> Reply; 2] = [
        || Reply::json(429, "{}").with_header("retry-after", "1"),
        || Reply::json(503, "{}").with_header("retry-after", "1"),
    ];
    for reply in replies {
        let server = TestServer::start(move |_| reply());
        let api = api(&server.url);

        let failure = GitLabChanges::new(&api)
            .open_page(&WIDGETS, &budget())
            .await
            .unwrap_err();

        assert_eq!(failure.kind(), FailureKind::Throttled);
        assert_eq!(server.requests().len(), 1);
    }
}

/// The whole lookup over GitLab: a project whose open merge requests fit on one page costs
/// one request, and its stacks come from it.
#[tokio::test]
async fn a_project_on_one_page_costs_one_request() {
    let server = gitlab(100, four);
    let api = api(&server.url);
    let lookup = GitLabChanges::new(&api);
    let budget = budget();
    let at = chrono::Utc::now();

    let report = change_stacks(
        [
            (
                ChangeRef {
                    repo: WIDGETS,
                    number: 2,
                },
                at,
            ),
            (
                ChangeRef {
                    repo: WIDGETS,
                    number: 4,
                },
                at,
            ),
        ],
        &lookup,
        &budget,
    )
    .await;

    assert_eq!(server.requests().len(), 1);
    let stack = &report.stacks[&ChangeRef {
        repo: WIDGETS,
        number: 2,
    }];
    assert_eq!(stack.current().number, 2);
    let below: Vec<&StackedChange> = stack.below().iter().collect();
    assert_eq!(
        below.iter().map(|change| change.number).collect::<Vec<_>>(),
        [1]
    );
    assert_eq!(
        stack.bottom(),
        &ChangeStackBottom::Reached {
            branch: "main".to_string()
        }
    );
    let above: &ChangeStackAbove = stack.above();
    assert_eq!(above.changes[0].change.number, 3);
    assert_eq!(above.changes[0].above.changes[0].change.number, 4);
    assert_eq!(above.end, ChangeStackEnd::Complete);
    assert_eq!(
        stack.current().web_url.as_deref(),
        Some(format!("{}/acme/widgets/-/merge_requests/2", server.url).as_str())
    );
}

/// The same stacks when the project takes several pages and branches are asked one by one.
#[tokio::test]
async fn a_project_of_several_pages_is_asked_per_branch() {
    let whole = gitlab(100, four);
    let paged = gitlab(1, four);
    let (api_whole, api_paged) = (api(&whole.url), api(&paged.url));
    let at = chrono::Utc::now();
    let wanted = [(
        ChangeRef {
            repo: WIDGETS,
            number: 2,
        },
        at,
    )];

    let from_page = change_stacks(wanted.clone(), &GitLabChanges::new(&api_whole), &budget()).await;
    let budget = budget();
    let per_branch = change_stacks(wanted, &GitLabChanges::new(&api_paged), &budget).await;

    // The same but for the links, which name each server.
    let shape = |stacks: &std::collections::HashMap<ChangeRef<u64>, crate::model::ChangeStack>,
                 server: &TestServer| {
        serde_json::to_string(
            &stacks[&ChangeRef {
                repo: WIDGETS,
                number: 2,
            }],
        )
        .unwrap()
        .replace(&server.url, "https://gitlab.example.org")
    };
    assert_eq!(
        shape(&from_page.stacks, &whole),
        shape(&per_branch.stacks, &paged)
    );
    assert_eq!(per_branch.read_in_part, 1);
    assert!(budget.spent() > 1);
}

/// `jdoe`'s events in March 2026: opening `!2`, a comment on `!4`, and a push.
fn events() -> Value {
    json!([
        {
            "id": 501, "action_name": "opened", "created_at": "2026-03-05T09:00:00Z",
            "project_id": WIDGETS, "target_type": "MergeRequest", "target_iid": 2,
            "target_title": "Parse step 2",
        },
        {
            "id": 502, "action_name": "commented on", "created_at": "2026-03-06T09:00:00Z",
            "project_id": WIDGETS, "target_type": "Note", "target_iid": 9001,
            "note": { "body": "Looks good", "noteable_type": "MergeRequest", "noteable_iid": 4 },
        },
        {
            "id": 503, "action_name": "pushed to", "created_at": "2026-03-07T09:00:00Z",
            "project_id": WIDGETS,
            "push_data": { "ref": "feature/parse-b", "commit_count": 1, "commit_to": "abc123" },
        },
    ])
}

fn march() -> CollectOptions {
    use chrono::TimeZone;
    CollectOptions {
        start: chrono::Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap(),
        end: chrono::Utc.with_ymd_and_hms(2026, 4, 1, 0, 0, 0).unwrap(),
    }
}

/// A checked GitLab connection named `name` (its log scope is `gitlab/{name}`).
async fn runtime(server: &TestServer, name: &str) -> ProviderRuntime {
    let connection = Connection::new(
        ConnectionId::new(Source::Gitlab, name),
        name,
        ConnectionLocation::Local,
        ConnectionValues::new(
            [
                ("GITLAB_URL".to_string(), Some(server.url.clone())),
                ("GITLAB_TOKEN".to_string(), Some(TOKEN.to_string())),
                ("GITLAB_USER".to_string(), Some("jdoe".to_string())),
            ]
            .into(),
        ),
    );
    let runtime = ProviderRuntime::inspect(Arc::new(GitLabProvider), &connection).await;
    assert!(runtime.usable(), "{:?}", runtime.info().reason);
    runtime
}

/// An event of `type` on merge request `mr` of `project`.
fn event(id: &str, entry_type: GitLabType, project: Option<u64>, mr: Option<u64>) -> ActivityEntry {
    ActivityEntry::GitLab(GitLabEntry {
        source: GitLabTag,
        connection: ConnectionName::unstamped(),
        id: id.to_string(),
        entry_type,
        timestamp: "2026-03-05T09:00:00+00:00".into(),
        title: None,
        target_type: None,
        project_id: project,
        mr_iid: mr,
        issue_iid: None,
        branch: None,
        commit_count: None,
        note_body: None,
        note_file: None,
        note_resolved: None,
        pushed_commit: None,
        change_stack: None,
    })
}

/// Each GitLab entry's stack, by id.
fn stacks(entries: &[ActivityEntry]) -> BTreeMap<String, Option<Vec<u64>>> {
    entries
        .iter()
        .filter_map(|entry| match entry {
            ActivityEntry::GitLab(entry) => Some((
                entry.id.clone(),
                entry.change_stack.as_ref().map(|stack| {
                    let mut members: Vec<u64> =
                        stack.below().iter().map(|change| change.number).collect();
                    members.push(stack.current().number);
                    members
                }),
            )),
            _ => None,
        })
        .collect()
}

/// A stack, as `stacks` shows one, that no lookup made: it must not survive an annotation.
fn stale(entry: &mut ActivityEntry, from: &ActivityEntry) {
    let (ActivityEntry::GitLab(entry), ActivityEntry::GitLab(from)) = (entry, from) else {
        panic!("GitLab entries");
    };
    entry.change_stack = from.change_stack.clone();
}

fn lines(scope: &str) -> Vec<crate::model::LogEntry> {
    crate::diagnostics::entries()
        .into_iter()
        .filter(|line| line.scope == scope)
        .collect()
}

/// Every GitLab entry gets the stack of the merge request it is about, as GitLab has it now:
/// a comment as its merge request does, and a push, an event about no open merge request or
/// about none at all gets none, whatever it held before. Other kinds are left as they are.
#[tokio::test]
async fn every_entry_gets_its_merge_requests_stack_and_no_other() {
    let server = gitlab(100, four);
    let runtime = runtime(&server, "stack-annotate").await;
    let system = ActivityEntry::System(SystemEntry {
        source: SystemTag,
        connection: ConnectionName::unstamped(),
        id: "unlock".to_string(),
        entry_type: SystemEntryType::Unlock,
        timestamp: "2026-03-05T08:00:00+00:00".into(),
        label: "Unlocked".to_string(),
    });
    let mut entries = vec![
        event("opened", GitLabType::Opened, Some(WIDGETS), Some(2)),
        event("comment", GitLabType::CommentedOn, Some(WIDGETS), Some(4)),
        event("push", GitLabType::PushedTo, Some(WIDGETS), Some(2)),
        event("closed", GitLabType::Merged, Some(WIDGETS), Some(9)),
        event("no-project", GitLabType::Opened, None, Some(2)),
        event("issue", GitLabType::Opened, Some(WIDGETS), None),
        system.clone(),
    ];
    let requests_before = server.requests().len();

    runtime.annotate(&mut entries).await;
    // What an earlier collection would have left: every entry with the stack of `!2`.
    let annotated = entries[0].clone();
    for entry in &mut entries[1..6] {
        stale(entry, &annotated);
    }
    runtime.annotate(&mut entries).await;

    assert_eq!(
        stacks(&entries),
        BTreeMap::from([
            ("opened".to_string(), Some(vec![1, 2])),
            ("comment".to_string(), Some(vec![1, 2, 3, 4])),
            ("push".to_string(), None),
            ("closed".to_string(), None),
            ("no-project".to_string(), None),
            ("issue".to_string(), None),
        ])
    );
    assert_eq!(format!("{:?}", entries[6]), format!("{system:?}"));
    // One page for the project, each time.
    assert_eq!(server.requests().len() - requests_before, 2);
    let summary = lines("gitlab/stack-annotate");
    assert!(
        summary.iter().any(|line| line.level == LogLevel::Info),
        "{summary:?}"
    );
}

/// A stack that is gone (its merge requests merged) is cleared by the next annotation.
#[tokio::test]
async fn a_stack_no_longer_there_is_cleared() {
    let merged = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let server = gitlab(100, {
        let merged = merged.clone();
        move |base| {
            if merged.load(std::sync::atomic::Ordering::SeqCst) {
                BTreeMap::from([(WIDGETS, Vec::new())])
            } else {
                four(base)
            }
        }
    });
    let runtime = runtime(&server, "stack-cleared").await;
    let mut entries = vec![event("opened", GitLabType::Opened, Some(WIDGETS), Some(2))];

    runtime.annotate(&mut entries).await;
    assert_eq!(stacks(&entries)["opened"], Some(vec![1, 2]));
    merged.store(true, std::sync::atomic::Ordering::SeqCst);
    runtime.annotate(&mut entries).await;

    assert_eq!(stacks(&entries)["opened"], None);
}

/// A lookup that fails keeps every entry, shows no stack and says why in the Logs.
#[tokio::test]
async fn a_failed_lookup_keeps_the_entries_without_stacks() {
    let server = TestServer::start(|request| {
        if request.path().ends_with("/user") {
            Reply::json(200, r#"{"id":7,"username":"jdoe"}"#)
        } else {
            Reply::json(500, r#"{"message":"500 database is down"}"#)
        }
    });
    let runtime = runtime(&server, "stack-failed").await;
    let mut entries = vec![
        event("opened", GitLabType::Opened, Some(WIDGETS), Some(2)),
        event("comment", GitLabType::CommentedOn, Some(WIDGETS), Some(4)),
    ];

    runtime.annotate(&mut entries).await;

    assert_eq!(
        stacks(&entries),
        BTreeMap::from([("opened".to_string(), None), ("comment".to_string(), None)])
    );
    let warnings: Vec<_> = lines("gitlab/stack-failed")
        .into_iter()
        .filter(|line| line.level == LogLevel::Warn)
        .collect();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        warnings[0].message.contains("database is down"),
        "{warnings:?}"
    );
}

/// Stacks are looked up after the day cache: a cache file never holds one, and days the cache
/// serves get them all the same.
#[tokio::test]
async fn stacks_are_never_cached_and_cached_days_get_them() {
    let server = gitlab(100, four);
    let runtime = runtime(&server, "stack-cache").await;
    let cache = tempfile::tempdir().unwrap();
    let events_asked = || {
        server
            .requests()
            .iter()
            .filter(|request| request.path().ends_with("/events"))
            .count()
    };

    let mut first = runtime
        .collect_in(cache.path(), march(), None)
        .await
        .unwrap();
    runtime.annotate(&mut first).await;
    let mut cached = runtime
        .collect_in(cache.path(), march(), None)
        .await
        .unwrap();
    let file_text: String = std::fs::read_dir(cache.path())
        .unwrap()
        .map(|file| std::fs::read_to_string(file.unwrap().path()).unwrap_or_default())
        .collect();
    runtime.annotate(&mut cached).await;

    assert_eq!(
        events_asked(),
        1,
        "the second collection came from the cache"
    );
    assert!(file_text.contains("projectId"), "{file_text}");
    assert!(!file_text.contains("changeStack"), "{file_text}");
    let expected = BTreeMap::from([
        ("501".to_string(), Some(vec![1, 2])),
        ("502".to_string(), Some(vec![1, 2, 3, 4])),
        ("503".to_string(), None),
    ]);
    assert_eq!(stacks(&first), expected);
    assert_eq!(stacks(&cached), expected);
}
