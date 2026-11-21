use std::{
    cell::Cell,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use serde::Deserialize;
use serde_json::json;

use super::{PageError, PagedRow, Pager};
use crate::provider::{
    gitlab::{GitLabConfig, api::GitLabApi},
    remote_api::{
        RequestBudget, Retry, Unmetered,
        test_server::{Reply, TestServer},
    },
    remote_error::ApiError,
};

#[derive(Debug, Deserialize)]
struct Row {
    id: u64,
}

impl PagedRow for Row {
    fn row_id(&self) -> u64 {
        self.id
    }
}

/// A budget of `left` requests that counts what it was asked for.
struct Counted {
    left: Cell<usize>,
}

#[derive(Debug, PartialEq, Eq)]
struct Spent;

impl RequestBudget for Counted {
    type Refusal = Spent;

    fn try_spend(&self) -> Result<(), Spent> {
        let left = self.left.get().checked_sub(1).ok_or(Spent)?;
        self.left.set(left);
        Ok(())
    }
}

fn api(url: &str) -> GitLabApi {
    GitLabApi::new(&GitLabConfig {
        url: url.to_string(),
        token: "glpat-example-token".to_string(),
        user: "jdoe".to_string(),
    })
    .unwrap()
}

/// Every page is numbered by `x-next-page`, without end, and holds a row of its own.
fn endless() -> TestServer {
    TestServer::start(|request| {
        let page: u64 = request.query("page").unwrap().parse().unwrap();
        Reply::json(200, json!([{ "id": page }]).to_string())
            .with_header("x-next-page", &(page + 1).to_string())
    })
}

fn first_page(api: &GitLabApi) -> reqwest::Url {
    let mut url = api.endpoint(&["projects", "42", "merge_requests"]);
    url.query_pairs_mut().append_pair("page", "1");
    url
}

/// The budget is asked before each page is sent; once it refuses, nothing more is sent.
#[tokio::test]
async fn each_page_is_paid_for_before_it_is_sent() {
    let server = endless();
    let api = api(&server.url);
    let budget = Counted { left: Cell::new(2) };
    let mut pager = Pager::new(&api, first_page(&api), Retry::Never, &budget);

    let mut ids = Vec::new();
    let error = loop {
        match pager.next_page::<Row>().await.unwrap() {
            Ok(page) => ids.extend(page.rows.iter().map(|row| row.id)),
            Err(error) => break error,
        }
    };

    assert_eq!(ids, [1, 2]);
    assert_eq!(error, PageError::Refused(Spent));
    assert_eq!(server.requests().len(), 2);
    assert!(
        pager.next_page::<Row>().await.is_none(),
        "nothing after an error"
    );
}

/// A page that names itself as the next is not asked again.
#[tokio::test]
async fn a_page_asked_before_is_repeated() {
    let server = TestServer::start(|_| {
        Reply::json(200, json!([{ "id": 1 }]).to_string()).with_header("x-next-page", "1")
    });
    let api = api(&server.url);
    let mut pager = Pager::new(&api, first_page(&api), Retry::OnceShort, &Unmetered);

    assert_eq!(
        pager.next_page::<Row>().await.unwrap().unwrap().rows.len(),
        1
    );
    assert_eq!(
        pager.next_page::<Row>().await.unwrap().unwrap_err(),
        PageError::Repeated
    );
    assert_eq!(server.requests().len(), 1);
}

/// A fresh page of rows already read is repeated too; a row read again beside a new one is
/// left out and the new one kept.
#[tokio::test]
async fn a_page_of_rows_already_read_is_repeated() {
    let server = TestServer::start(|request| {
        let (rows, next) = match request.query("page").as_deref() {
            Some("1") => (json!([{ "id": 1 }, { "id": 2 }]), "2"),
            Some("2") => (json!([{ "id": 2 }, { "id": 3 }]), "3"),
            _ => (json!([{ "id": 1 }]), "4"),
        };
        Reply::json(200, rows.to_string()).with_header("x-next-page", next)
    });
    let api = api(&server.url);
    let mut pager = Pager::new(&api, first_page(&api), Retry::OnceShort, &Unmetered);
    let ids = |page: super::Page<Row>| page.rows.iter().map(|row| row.id).collect::<Vec<_>>();

    assert_eq!(ids(pager.next_page().await.unwrap().unwrap()), [1, 2]);
    assert_eq!(ids(pager.next_page().await.unwrap().unwrap()), [3]);
    assert_eq!(
        pager.next_page::<Row>().await.unwrap().unwrap_err(),
        PageError::Repeated
    );
}

/// Rows of another shape are counted, not read; a page of only such rows is no repeat.
#[tokio::test]
async fn rows_of_another_shape_are_skipped_and_counted() {
    let server = TestServer::start(|request| match request.query("page").as_deref() {
        Some("1") => Reply::json(200, json!([{ "id": 1 }, { "id": "x" }, {}]).to_string())
            .with_header("x-next-page", "2"),
        _ => Reply::json(200, json!([{ "name": "x" }]).to_string()).with_header("x-next-page", ""),
    });
    let api = api(&server.url);
    let mut pager = Pager::new(&api, first_page(&api), Retry::OnceShort, &Unmetered);

    let first = pager.next_page::<Row>().await.unwrap().unwrap();
    assert_eq!((first.rows.len(), first.skipped), (1, 2));
    let second = pager.next_page::<Row>().await.unwrap().unwrap();
    assert_eq!((second.rows.len(), second.skipped), (0, 1));
    assert!(pager.next_page::<Row>().await.is_none());
}

/// The pager asks as its caller says: a caller that never retries gets the busy answer from one
/// request, a page that is no list fails.
#[tokio::test]
async fn failures_are_the_requests_own() {
    let calls = Arc::new(AtomicUsize::new(0));
    let server = TestServer::start({
        let calls = calls.clone();
        move |request| {
            calls.fetch_add(1, Ordering::SeqCst);
            match request.path() {
                "/api/v4/busy" => Reply::json(429, "{}").with_header("Retry-After", "1"),
                _ => Reply::json(200, r#"{"message":"no list"}"#),
            }
        }
    });
    let api = api(&server.url);

    let mut busy = Pager::new(&api, api.endpoint(&["busy"]), Retry::Never, &Unmetered);
    assert!(matches!(
        busy.next_page::<Row>().await.unwrap().unwrap_err(),
        PageError::Request(ApiError::Status {
            retry_after: Some(1),
            ..
        })
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    let mut no_list = Pager::new(&api, api.endpoint(&["object"]), Retry::Never, &Unmetered);
    assert_eq!(
        no_list.next_page::<Row>().await.unwrap().unwrap_err(),
        PageError::Request(ApiError::Decode)
    );
}

/// After one page the pager tells whether GitLab named another: a first page is then the whole
/// list or only part of it.
#[tokio::test]
async fn the_pager_tells_whether_a_page_follows() {
    let server = TestServer::start(|request| match request.query("page").as_deref() {
        Some("1") => {
            Reply::json(200, json!([{ "id": 1 }]).to_string()).with_header("x-next-page", "2")
        }
        _ => Reply::json(200, json!([{ "id": 2 }]).to_string()).with_header("x-next-page", ""),
    });
    let api = api(&server.url);
    let mut pager = Pager::new(&api, first_page(&api), Retry::Never, &Unmetered);

    assert!(pager.has_next(), "the first page is still to read");
    pager.next_page::<Row>().await.unwrap().unwrap();
    assert!(pager.has_next());
    pager.next_page::<Row>().await.unwrap().unwrap();
    assert!(!pager.has_next());
}
