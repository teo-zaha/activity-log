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
        remote_error::{ApiError, RemoteFailure},
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

    let mut entry = map_gitlab_event(serde_json::from_value(event).unwrap());
    entry.connection.stamp(&"work".into());
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
        "id": 4712,
        "action_name": "commented on",
        "created_at": "2026-03-01T10:30:00Z",
        "target_type": "Note",
        "push_data": { "commit_to": "abc123" }
    });

    let mut entry = map_gitlab_event(serde_json::from_value(event).unwrap());
    entry.connection.stamp(&"work".into());
    assert_eq!(entry.pushed_commit, None);
    assert!(
        serde_json::to_value(&entry)
            .unwrap()
            .get("pushedCommit")
            .is_none()
    );
}

/// GitLab sends `null` for what a push does not have; the entry leaves it out instead of
/// sending `""` or `"unknown"` for it.
#[test]
fn a_push_has_no_title_and_no_target() {
    let event = json!({
        "id": 4713,
        "action_name": "pushed to",
        "created_at": "2026-03-01T10:30:00Z",
        "target_title": null,
        "target_type": null,
        "target_iid": null,
        "push_data": { "ref": "main", "commit_count": 1, "commit_to": "abc123" },
        "note": null
    });

    let mut entry = map_gitlab_event(serde_json::from_value(event).unwrap());
    entry.connection.stamp(&"work".into());
    let json = serde_json::to_value(&entry).unwrap();
    assert!(json.get("title").is_none());
    assert!(json.get("targetType").is_none());
    assert_eq!(json["timestamp"], "2026-03-01T10:30:00Z");
}

/// A row that is no event (here one without its creation time) is skipped, and counted once in
/// the Logs pane; the rows around it are kept.
#[tokio::test]
async fn a_row_that_is_no_event_is_skipped_and_the_others_kept() {
    let server = TestServer::start(|_| {
        Reply::json(
            200,
            json!([
                { "id": 1, "action_name": "opened", "created_at": "2026-03-02T10:00:00Z" },
                { "id": 2, "action_name": "opened" },
                { "id": 3, "action_name": "opened", "created_at": "yesterday" },
            ])
            .to_string(),
        )
    });

    let entries = collect_gitlab(&march(), &context(&server.url, "jdoe"))
        .await
        .unwrap();

    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>(),
        ["1"]
    );
    assert!(
        crate::diagnostics::entries()
            .iter()
            .any(|entry| entry.message.contains("Skipped 2 GitLab events"))
    );
}

/// Pages follow GitLab's `x-next-page`, whatever a page holds: a short page that names a next
/// one leads to it, and an empty `x-next-page` ends the list.
#[tokio::test]
async fn pages_follow_the_next_page_gitlab_names() {
    let event = |id: u32| json!({ "id": id, "action_name": "opened", "created_at": "2026-03-02T10:00:00Z" });
    let server = TestServer::start(move |request| match request.query("page").as_deref() {
        Some("1") => Reply::json(200, json!([event(1), { "id": 2 }]).to_string())
            .with_header("x-next-page", "2"),
        _ => Reply::json(200, json!([event(3)]).to_string()).with_header("x-next-page", ""),
    });

    let entries = collect_gitlab(&march(), &context(&server.url, "jdoe"))
        .await
        .unwrap();

    assert_eq!(server.requests().len(), 2);
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>(),
        ["1", "3"]
    );
}

/// Where GitLab names the next page only in a `Link` header (keyset pages), that page is read.
#[tokio::test]
async fn a_link_header_names_the_next_page() {
    let base = std::sync::Arc::new(std::sync::OnceLock::<String>::new());
    let server = TestServer::start({
        let base = base.clone();
        move |request| {
            if request.query("cursor").is_some() {
                return Reply::json(200, "[]");
            }
            let next = format!(
                "<{}{}?cursor=abc>; rel=\"next\"",
                base.get().unwrap(),
                request.path()
            );
            Reply::json(200, "[]").with_header("link", &next)
        }
    });
    base.set(server.url.clone()).unwrap();

    collect_gitlab(&march(), &context(&server.url, "jdoe"))
        .await
        .unwrap();

    assert_eq!(server.requests().len(), 2);
    assert_eq!(server.requests()[1].query("cursor").as_deref(), Some("abc"));
}

/// A page that names itself as the next, or sends the same events again, stops the collection
/// instead of reading until it times out.
#[tokio::test]
async fn a_repeating_page_stops_the_collection() {
    let event = json!({ "id": 7, "action_name": "opened", "created_at": "2026-03-02T10:00:00Z" });
    let itself = TestServer::start(move |_| {
        Reply::json(200, json!([event.clone()]).to_string()).with_header("x-next-page", "1")
    });
    let error = collect_gitlab(&march(), &context(&itself.url, "jdoe"))
        .await
        .unwrap_err();
    assert!(error.is::<super::PagesRepeated>(), "{error:#}");

    let event = json!({ "id": 8, "action_name": "opened", "created_at": "2026-03-02T10:00:00Z" });
    let again = TestServer::start(move |request| {
        let page = request.query("page").unwrap_or_default();
        let next = (page.parse::<u32>().unwrap() + 1).to_string();
        Reply::json(200, json!([event.clone()]).to_string()).with_header("x-next-page", &next)
    });
    let error = collect_gitlab(&march(), &context(&again.url, "jdoe"))
        .await
        .unwrap_err();
    assert!(error.is::<super::PagesRepeated>(), "{error:#}");
    assert_eq!(again.requests().len(), 2);
}

/// An answer that is no list of rows is no page: the collection fails with it.
#[tokio::test]
async fn an_answer_that_is_no_list_fails_the_collection() {
    for body in [r#"{"message":"oops"}"#, "[{"] {
        let server = TestServer::start(move |_| Reply::json(200, body));
        let error = collect_gitlab(&march(), &context(&server.url, "jdoe"))
            .await
            .unwrap_err();
        assert_eq!(
            error
                .downcast_ref::<RemoteFailure>()
                .map(|failure| &failure.error),
            Some(&ApiError::Decode),
            "{body}"
        );
    }
}

/// GitLab's ids are 64-bit; an iid or count past 32 bits is kept, not truncated or refused.
#[test]
fn wide_ids_and_counts_are_kept() {
    let event = json!({
        "id": 4714,
        "action_name": "opened",
        "created_at": "2026-03-01T10:30:00Z",
        "target_type": "MergeRequest",
        "target_iid": 4_294_967_296_u64,
        "push_data": { "commit_count": 5_000_000_000_u64 }
    });
    let mut entry = map_gitlab_event(serde_json::from_value(event).unwrap());
    entry.connection.stamp(&"work".into());
    assert_eq!(entry.mr_iid, Some(4_294_967_296));
    assert_eq!(entry.commit_count, Some(5_000_000_000));
}

/// `target_iid` is the iid of whatever the event is about: an issue's is no merge request's.
#[test]
fn an_issue_is_no_merge_request() {
    let issue = json!({
        "id": 1, "action_name": "opened", "created_at": "2026-03-01T10:30:00Z",
        "target_type": "Issue", "target_iid": 53, "target_title": "Broken build"
    });
    let entry = map_gitlab_event(serde_json::from_value(issue).unwrap());
    assert_eq!((entry.mr_iid, entry.issue_iid), (None, Some(53)));

    // A comment names what it is on in its note.
    let comment = json!({
        "id": 2, "action_name": "commented on", "created_at": "2026-03-01T10:31:00Z",
        "target_type": "DiffNote", "target_iid": 9001,
        "note": { "body": "nit", "noteable_type": "MergeRequest", "noteable_iid": 12 }
    });
    let entry = map_gitlab_event(serde_json::from_value(comment).unwrap());
    assert_eq!((entry.mr_iid, entry.issue_iid), (Some(12), None));
}

/// A push that creates its branch is a push: it names its commit and folds into it.
#[test]
fn a_push_of_a_new_branch_names_its_commit() {
    let event = json!({
        "id": 3, "action_name": "pushed new", "created_at": "2026-03-01T10:30:00Z",
        "push_data": { "ref": "feature", "commit_count": 1, "commit_to": "abc123" }
    });
    let mut entry = map_gitlab_event(serde_json::from_value(event).unwrap());
    entry.connection.stamp(&"work".into());
    assert_eq!(entry.entry_type, GitLabType::PushedNew);
    assert_eq!(entry.pushed_commit.as_deref(), Some("abc123"));
}

/// An action GitLab adds later is kept as `unknown`, with what it says about its target.
#[test]
fn an_action_this_version_does_not_know_is_unknown() {
    let event = json!({
        "id": 4, "action_name": "teleported", "created_at": "2026-03-01T10:30:00Z",
        "target_type": "Issue", "target_iid": 7
    });
    let mut entry = map_gitlab_event(serde_json::from_value(event).unwrap());
    entry.connection.stamp(&"work".into());
    assert_eq!(entry.entry_type, GitLabType::Unknown);
    assert_eq!(entry.issue_iid, Some(7));
}

/// Fresh page URLs that bring back events already collected (`1, 2, 1`) stop the collection: the
/// guard looks at every event of this collection, not only the page before.
#[tokio::test]
async fn a_page_of_events_already_collected_stops_the_collection() {
    let server = TestServer::start(|request| {
        let page = request.query("page").unwrap_or_default();
        let id = match page.as_str() {
            "1" => 1,
            "2" => 2,
            _ => 1,
        };
        let next = (page.parse::<u32>().unwrap() + 1).to_string();
        Reply::json(
            200,
            json!([{ "id": id, "action_name": "opened", "created_at": "2026-03-02T10:00:00Z" }])
                .to_string(),
        )
        .with_header("x-next-page", &next)
    });

    let error = collect_gitlab(&march(), &context(&server.url, "jdoe"))
        .await
        .unwrap_err();

    assert!(error.is::<super::PagesRepeated>(), "{error:#}");
    assert_eq!(server.requests().len(), 3);
}

/// A next page named on a second `Link` header line is followed like one on the first.
#[tokio::test]
async fn every_link_header_line_is_read() {
    let base = std::sync::Arc::new(std::sync::OnceLock::<String>::new());
    let server = TestServer::start({
        let base = base.clone();
        move |request| {
            if request.query("cursor").is_some() {
                return Reply::json(200, "[]");
            }
            let first = format!(
                "<{}{}?page=1>; rel=\"first\"",
                base.get().unwrap(),
                request.path()
            );
            let next = format!(
                "<{}{}?cursor=abc>; rel=\"next\"",
                base.get().unwrap(),
                request.path()
            );
            Reply::json(200, "[]")
                .with_header("link", &first)
                .with_header("link", &next)
        }
    });
    base.set(server.url.clone()).unwrap();

    collect_gitlab(&march(), &context(&server.url, "jdoe"))
        .await
        .unwrap();

    assert_eq!(server.requests().len(), 2);
}

/// An event names its project (`project_id`): a merge request's iid means something only in
/// its project. GitLab's `null` leaves it absent, never `0`.
#[test]
fn an_event_names_its_project() {
    let comment = json!({
        "id": 5, "action_name": "commented on", "created_at": "2026-03-01T10:30:00Z",
        "project_id": 42, "target_type": "DiffNote",
        "note": { "body": "nit", "noteable_type": "MergeRequest", "noteable_iid": 12 }
    });
    let mut entry = map_gitlab_event(serde_json::from_value(comment).unwrap());
    entry.connection.stamp(&"work".into());
    assert_eq!((entry.project_id, entry.mr_iid), (Some(42), Some(12)));
    assert_eq!(serde_json::to_value(&entry).unwrap()["projectId"], 42);

    let without = json!({
        "id": 6, "action_name": "opened", "created_at": "2026-03-01T10:30:00Z",
        "project_id": null
    });
    let mut entry = map_gitlab_event(serde_json::from_value(without).unwrap());
    entry.connection.stamp(&"work".into());
    assert_eq!(entry.project_id, None);
    assert!(
        serde_json::to_value(&entry)
            .unwrap()
            .get("projectId")
            .is_none()
    );
}
