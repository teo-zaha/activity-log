use reqwest::StatusCode;
use serde_json::{Value, json};

use super::*;
use crate::{
    model::{ProviderProblem, Timestamp},
    provider::remote_api::test_server::{Reply, TestServer},
};

const TOKEN: &str = "test-figma-token";

fn config() -> FigmaConfig {
    FigmaConfig {
        token: TOKEN.to_owned(),
        files: None,
    }
}

fn file() -> FigmaFileKey {
    FigmaFileKey::from_path("/design/FILEkey0001").unwrap()
}

#[test]
fn uses_the_fixed_api_origin() {
    let url = FigmaApi::new(&config())
        .unwrap()
        .file_endpoint(&file(), "comments");
    assert_eq!(
        url.as_str(),
        "https://api.figma.com/v1/files/FILEkey0001/comments"
    );
}

#[tokio::test]
async fn reads_identity_metadata_and_comments_with_the_figma_header() {
    let server = TestServer::start(|request| {
        let answer = match request.path() {
            "/v1/me" => json!({"id":"1001","handle":"ada"}),
            "/v1/files/FILEkey0001/meta" => json!({
                "file": {
                    "name":"Checkout",
                    "editorType":"figma",
                    "last_touched_at":"2026-11-23T08:00:00Z",
                    "creator":{"id":"1001","handle":"ada","img_url":"https://example.org/ada.png"},
                    "url":"https://www.figma.com/design/FILEkey0001/Checkout"
                }
            }),
            "/v1/files/FILEkey0001/comments" => json!({
                "comments": [{
                    "id":"13",
                    "file_key":"FILEkey0001",
                    "parent_id":"11",
                    "user":{"id":"1001","handle":"ada","img_url":"https://example.org/ada.png"},
                    "created_at":"2026-11-23T08:30:00.123Z",
                    "resolved_at":null,
                    "message":"Checked the title",
                    "client_meta":{"node_id":"3:4","node_offset":{"x":0,"y":0}},
                    "reactions":[],
                    "order_id":null
                }]
            }),
            _ => return Reply::json(404, "{}"),
        };
        Reply::json(200, answer.to_string())
    });
    let api = FigmaApi::at(&server.url, &config()).unwrap();
    let file = file();

    let identity = api.me().await.unwrap().unwrap();
    assert_eq!(identity.id, "1001");
    assert_eq!(identity.username, "ada");

    let meta = api.file_meta(&file).await.unwrap();
    assert_eq!(meta.name, "Checkout");
    assert_eq!(meta.editor_type, "figma");
    assert_eq!(meta.last_touched_at, "2026-11-23T08:00:00Z");
    assert_eq!(
        meta.url.as_deref(),
        Some("https://www.figma.com/design/FILEkey0001/Checkout")
    );

    let comments = api.comments(&file).await.unwrap();
    let [comment] = comments.as_slice() else {
        panic!("expected one comment");
    };
    assert_eq!(comment.id, "13");
    assert_eq!(comment.user.id, "1001");
    assert_eq!(comment.created_at, "2026-11-23T08:30:00.123Z");
    assert_eq!(comment.parent_id.as_deref(), Some("11"));
    assert_eq!(comment.message, "Checked the title");
    assert!(comment.resolved_at.is_none());
    assert_eq!(comment.client_meta.node_id.as_deref(), Some("3:4"));

    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    for request in requests {
        assert_eq!(request.header("x-figma-token"), Some(TOKEN));
        assert_eq!(request.header("authorization"), None);
    }
}

#[tokio::test]
async fn does_not_follow_a_redirect_to_another_origin() {
    let elsewhere = TestServer::start(|_| Reply::json(200, r#"{"id":"2002"}"#));
    let location = format!("{}/v1/me", elsewhere.url);
    let server = TestServer::start(move |_| Reply::redirect(location.clone()));
    let api = FigmaApi::at(&server.url, &config()).unwrap();

    assert!(matches!(
        api.me().await,
        Err(ApiError::Status {
            status: StatusCode::FOUND,
            ..
        })
    ));
    assert!(elsewhere.requests().is_empty());
}

fn version(id: &str, created_at: &str) -> Value {
    json!({
        "id": id,
        "created_at": created_at,
        "label": null,
        "description": null,
        "user": {"id":"1001","handle":"ada","img_url":"https://example.org/ada.png"}
    })
}

fn page(rows: Vec<Value>, has_next: bool) -> Reply {
    let pagination = if has_next {
        json!({"next_page": "https://example.org/other"})
    } else {
        json!({})
    };
    Reply::json(
        200,
        json!({"versions": rows, "pagination": pagination}).to_string(),
    )
}

const START: &str = "2026-11-23T09:00:00Z";
const AFTER: &str = "2026-11-23T10:00:00Z";
const BEFORE: &str = "2026-11-23T08:00:00Z";

#[tokio::test]
async fn versions_walk_the_oldest_cursor_and_stop_after_crossing_the_start() {
    let server = TestServer::start(|request| match request.query("before").as_deref() {
        None => page(vec![version("143", AFTER), version("00142", START)], true),
        Some("00142") => page(vec![version("00142", START), version("141", BEFORE)], true),
        _ => Reply::json(404, "{}"),
    });
    let api = FigmaApi::at(&server.url, &config()).unwrap();

    let rows = api
        .versions_since(&file(), Timestamp::from(START).utc())
        .await
        .unwrap();
    let ids: Vec<_> = rows.iter().map(|row| row.id.as_str()).collect();
    assert_eq!(ids, ["143", "00142"]);
    let oldest = &rows[1];
    assert_eq!(oldest.user.id, "1001");
    assert_eq!(oldest.created_at, START);
    assert!(oldest.label.is_none() && oldest.description.is_none());

    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    for request in &requests {
        assert_eq!(request.path(), "/v1/files/FILEkey0001/versions");
        assert_eq!(request.query("page_size").as_deref(), Some("50"));
    }
    assert_eq!(requests[1].query("before").as_deref(), Some("00142"));
}

#[tokio::test]
async fn versions_stop_on_an_empty_page_or_without_a_next_page() {
    for (empty, next) in [(true, true), (false, false)] {
        let server = TestServer::start(move |_| {
            let rows = if empty {
                vec![]
            } else {
                vec![version("143", AFTER)]
            };
            page(rows, next)
        });
        let api = FigmaApi::at(&server.url, &config()).unwrap();
        let rows = api
            .versions_since(&file(), Timestamp::from(START).utc())
            .await
            .unwrap();
        assert_eq!(rows.is_empty(), empty);
        assert_eq!(server.requests().len(), 1);
    }
}

#[tokio::test]
async fn versions_fail_on_repeated_rows_or_a_repeated_cursor() {
    for cursor_repeated in [false, true] {
        let server = TestServer::start(move |request| {
            let mut rows = vec![version("142", START)];
            if cursor_repeated && request.query("before").is_some() {
                rows.insert(0, version("143", AFTER));
            }
            page(rows, true)
        });
        let api = FigmaApi::at(&server.url, &config()).unwrap();

        assert!(matches!(
            api.versions_since(&file(), Timestamp::from(START).utc())
                .await,
            Err(VersionsError::Repeated)
        ));
        assert_eq!(server.requests().len(), 2);
    }
}

#[tokio::test]
async fn refuses_a_version_id_that_cannot_be_a_numeric_cursor() {
    let server = TestServer::start(|_| page(vec![version("two", START)], false));
    let api = FigmaApi::at(&server.url, &config()).unwrap();

    assert!(matches!(
        api.versions_since(&file(), Timestamp::from(START).utc())
            .await,
        Err(VersionsError::Request(ApiError::Decode))
    ));
}

#[tokio::test]
async fn figmas_err_field_is_reported_with_credentials_redacted() {
    let server = TestServer::start(|_| {
        Reply::json(
            403,
            json!({"status": 403, "err": format!("Invalid token {TOKEN}")}).to_string(),
        )
    });
    let api = FigmaApi::at(&server.url, &config()).unwrap();

    let error = api.me().await.unwrap_err();
    assert_eq!(error.problem(), Some(ProviderProblem::Refused));
    assert!(matches!(
        error,
        ApiError::Status { status: StatusCode::FORBIDDEN, message, .. }
            if message == "Invalid token [redacted]"
    ));
}

#[tokio::test]
async fn figma_uses_the_shared_short_retry_and_throttled_problem() {
    for (retry_after, count) in [("0", 2), ("60", 1)] {
        let server = TestServer::start(move |_| {
            Reply::json(429, "{}").with_header("Retry-After", retry_after)
        });
        let api = FigmaApi::at(&server.url, &config()).unwrap();

        let error = api.comments(&file()).await.unwrap_err();
        assert_eq!(error.problem(), Some(ProviderProblem::Throttled));
        assert_eq!(server.requests().len(), count);
    }
}
