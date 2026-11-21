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
    model::{ChangeStackAbove, ChangeStackBottom, ChangeStackEnd, StackedChange},
    provider::{
        gitlab::{GitLabConfig, api::GitLabApi},
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
/// `x-next-page`. Rows are made once the server's address is known.
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
        let ["projects", project, "merge_requests"] = segments.as_slice() else {
            return Reply::json(404, r#"{"message":"404 Not Found"}"#);
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

/// A row the request did not ask for (another state, project, number or branch) means the
/// server ignored a filter: the list fails, and no further page is read.
#[tokio::test]
async fn a_row_that_was_not_asked_for_fails_the_list() {
    let base = "https://gitlab.example.org";
    let mut closed = mr(base, 2, "feature/parse-b", "feature/parse-a");
    closed["state"] = json!("merged");
    let mut elsewhere = mr(base, 2, "feature/parse-b", "feature/parse-a");
    elsewhere["target_project_id"] = json!(FORK);
    let unasked = mr(base, 3, "feature/parse-b", "feature/parse-a");
    let other_branch = mr(base, 2, "feature/parse-x", "feature/parse-y");

    // Whether each question fails: its numbers (`!2`), its source branch (`feature/parse-b`)
    // and its target branch (`feature/parse-a`).
    for (name, row, fails) in [
        ("closed", closed, [true, true, true]),
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

    let found = GitLabChanges::new(&api)
        .open_into(&WIDGETS, "main", &budget())
        .await
        .unwrap();

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
    let shape = |report: &crate::collect::change_stack::StackReport<u64>, server: &TestServer| {
        serde_json::to_string(
            &report.stacks[&ChangeRef {
                repo: WIDGETS,
                number: 2,
            }],
        )
        .unwrap()
        .replace(&server.url, "https://gitlab.example.org")
    };
    assert_eq!(shape(&from_page, &whole), shape(&per_branch, &paged));
    assert_eq!(per_branch.read_in_part, 1);
    assert!(budget.spent() > 1);
}
