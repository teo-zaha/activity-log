use reqwest::StatusCode;
use serde_json::json;

use super::*;
use crate::provider::remote_api::test_server::{Reply, TestServer};

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

#[tokio::test]
async fn version_requests_use_the_supplied_checkpoint_as_before() {
    let server = TestServer::start(|request| {
        let answer = if request.query("before").is_none() {
            json!({
                "versions":[{
                    "id":"00142",
                    "created_at":"2026-11-23T09:00:00Z",
                    "label":null,
                    "description":null,
                    "user":{"id":"1001","handle":"ada","img_url":"https://example.org/ada.png"}
                }],
                "pagination":{"next_page":"https://example.org/other"}
            })
        } else {
            json!({"versions":[],"pagination":{}})
        };
        Reply::json(200, answer.to_string())
    });
    let api = FigmaApi::at(&server.url, &config()).unwrap();
    let file = file();

    let first = api.version_page(&file, None).await.unwrap();
    let [version] = first.versions.as_slice() else {
        panic!("expected one version");
    };
    assert_eq!(version.id, "00142");
    assert_eq!(version.user.id, "1001");
    assert_eq!(version.created_at, "2026-11-23T09:00:00Z");
    assert!(version.label.is_none() && version.description.is_none());
    assert!(first.pagination.next_page.is_some());

    let second = api.version_page(&file, Some(version)).await.unwrap();
    assert!(second.versions.is_empty());
    assert!(second.pagination.next_page.is_none());

    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    for request in &requests {
        assert_eq!(request.path(), "/v1/files/FILEkey0001/versions");
        assert_eq!(request.query("page_size").as_deref(), Some("50"));
    }
    assert_eq!(requests[1].query("before").as_deref(), Some("00142"));
}

#[tokio::test]
async fn refuses_a_version_id_that_cannot_be_a_numeric_cursor() {
    let server = TestServer::start(|_| {
        Reply::json(
            200,
            json!({
                "versions":[{
                    "id":"two",
                    "created_at":"2026-11-23T09:00:00Z",
                    "label":null,
                    "description":null,
                    "user":{"id":"1001","handle":"ada","img_url":"https://example.org/ada.png"}
                }],
                "pagination":{}
            })
            .to_string(),
        )
    });
    let api = FigmaApi::at(&server.url, &config()).unwrap();

    assert!(matches!(
        api.version_page(&file(), None).await,
        Err(ApiError::Decode)
    ));
}
