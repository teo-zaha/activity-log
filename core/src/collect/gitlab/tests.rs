use chrono::{TimeZone, Utc};
use serde_json::json;

use super::{collect_gitlab, map_gitlab_event};
use crate::{
    model::{CollectOptions, ConnectionId, GitLabType, Source},
    provider::{
        connection::{Connection, ConnectionLocation, ConnectionValues},
        gitlab::GitLabConfig,
        provider_config::ProviderConfigResolve,
        provider_context::ProviderContext,
        remote_api::test_server::{Reply, TestServer},
    },
};

const TOKEN: &str = "glpat-example-token";

fn context(url: &str, user: &str) -> ProviderContext {
    let connection = Connection::new(
        ConnectionId::new(Source::Gitlab, "work"),
        "Work GitLab",
        ConnectionLocation::Local,
        ConnectionValues::new(
            [
                ("GITLAB_URL".to_string(), Some(url.to_string())),
                ("GITLAB_TOKEN".to_string(), Some(TOKEN.to_string())),
                ("GITLAB_USER".to_string(), Some(user.to_string())),
            ]
            .into(),
        ),
    );
    let mut context = ProviderContext::for_connection(&connection, GitLabConfig::requirements());
    context.with_typed::<GitLabConfig>().unwrap();
    context
}

fn march() -> CollectOptions {
    CollectOptions {
        start: Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap(),
        end: Utc.with_ymd_and_hms(2026, 4, 1, 0, 0, 0).unwrap(),
    }
}

#[tokio::test]
async fn the_configured_user_is_one_path_segment() {
    let server = TestServer::start(|_| {
        Reply::json(
            200,
            json!([{ "id": 1, "action_name": "pushed to", "created_at": "2026-03-02T10:00:00Z" }])
                .to_string(),
        )
    });

    let entries = collect_gitlab(&march(), &context(&server.url, "jane/../../admin?x#y"))
        .await
        .unwrap();

    assert_eq!(entries.len(), 1);
    let requests = server.requests();
    assert_eq!(
        requests[0].path(),
        "/api/v4/users/jane%2F..%2F..%2Fadmin%3Fx%23y/events"
    );
    assert_eq!(requests[0].query("after").as_deref(), Some("2026-02-28"));
    assert_eq!(requests[0].query("before").as_deref(), Some("2026-04-01"));
    assert_eq!(requests[0].header("private-token"), Some(TOKEN));
}

#[tokio::test]
async fn a_dot_segment_user_is_refused_without_a_request() {
    let server = TestServer::start(|_| Reply::json(200, "[]"));
    for user in [
        ".",
        "..",
        ".\t.",
        "jdoe\tx",
        "jane\ndoe",
        "jane\r",
        "a\u{0}b",
    ] {
        let error = collect_gitlab(&march(), &context(&server.url, user))
            .await
            .unwrap_err();
        assert_eq!(error.to_string(), "GITLAB_USER must be a GitLab user name");
    }
    assert!(server.requests().is_empty());
}

#[tokio::test]
async fn the_token_never_follows_a_redirect_elsewhere() {
    let elsewhere = TestServer::start(|_| Reply::json(200, "[]"));
    let target = format!("{}/api/v4/users/jdoe/events", elsewhere.url);
    let server = TestServer::start(move |_| Reply::redirect(target.clone()));

    let error = collect_gitlab(&march(), &context(&server.url, "jdoe"))
        .await
        .unwrap_err();

    assert_eq!(
        error.to_string(),
        "GitLab redirected to another address; set the URL it redirects to"
    );
    assert!(elsewhere.requests().is_empty());
}

#[test]
fn maps_gitlab_event_fields() {
    let event = json!({
        "id": 4711,
        "action_name": "pushed to",
        "created_at": "2026-03-01T10:30:00Z",
        "target_title": "feature/activity",
        "target_type": "MergeRequest",
        "target_iid": 12,
        "push_data": { "ref": "main", "commit_count": 3, "commit_to": "abc123" },
        "note": { "body": "Looks good", "resolved": true, "position": { "new_path": "src/lib.rs" } }
    });

    let entry = map_gitlab_event(&event).unwrap();
    assert_eq!(entry.entry_type, GitLabType::PushedTo);
    assert_eq!(entry.branch.as_deref(), Some("main"));
    assert_eq!(entry.commit_count, Some(3));
    assert_eq!(entry.note_file.as_deref(), Some("src/lib.rs"));
    assert_eq!(entry.pushed_commit.as_deref(), Some("abc123"));
    assert_eq!(entry.id, "4711");
    let json = serde_json::to_value(&entry).unwrap();
    assert_eq!(json["pushedCommit"], "abc123");
    assert!(json.get("raw").is_none());
}

#[test]
fn only_a_push_names_the_commit_it_pushed() {
    let event = json!({
        "action_name": "commented on",
        "created_at": "2026-03-01T10:30:00Z",
        "target_type": "Note",
        "push_data": { "commit_to": "abc123" }
    });

    let entry = map_gitlab_event(&event).unwrap();
    assert_eq!(entry.pushed_commit, None);
    assert!(
        entry.id.starts_with("h:"),
        "an event without an id gets a stable one"
    );
    assert_eq!(map_gitlab_event(&event).unwrap().id, entry.id);
    assert!(
        serde_json::to_value(&entry)
            .unwrap()
            .get("pushedCommit")
            .is_none()
    );
}
