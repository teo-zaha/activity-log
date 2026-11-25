use serde_json::json;
use tokio::time::Instant;

use super::*;
use crate::{
    model::{ConnectionId, GitLabType, LogLevel, Source, Timestamp},
    provider::{
        remote_api::test_server::{Reply, TestServer},
        remote_error::ApiError,
    },
};

fn client(server: &TestServer) -> GitLabApi {
    GitLabApi::new(&GitLabConfig {
        url: server.url.clone(),
        token: "example-token".into(),
        user: "dev".into(),
    })
    .unwrap()
}

#[tokio::test]
async fn a_project_name_survives_an_unusable_url_and_invalid_replies_are_omitted() {
    let server = TestServer::start(|request| {
        let row = match request.path() {
            "/api/v4/projects/1" => json!({
                "id": 1, "path_with_namespace": "acme/one",
                "web_url": "https://another.example.org/acme/one",
            }),
            "/api/v4/projects/2" => json!({"id": 3, "path_with_namespace": "acme/wrong"}),
            "/api/v4/projects/3" => json!({"id": 3, "path_with_namespace": "  "}),
            "/api/v4/projects/4" => json!({
                "id": 4, "path_with_namespace": "acme/four",
                "web_url": "https://other.example.org/acme/four",
            }),
            _ => panic!("unexpected project"),
        };
        Reply::json(200, row.to_string())
    });
    let id = ConnectionId::new(Source::Gitlab, "project-elsewhere-test");
    let report = diagnostics::in_connection(id.clone(), async {
        let report = read_projects(
            [1, 2, 3, 4].into(),
            &client(&server),
            &Budget::for_collection(),
        )
        .await;
        report.log();
        report
    })
    .await;
    assert_eq!(report.projects.len(), 2);
    assert_eq!(report.projects[&1].name, "acme/one");
    assert!(report.projects[&1].web_base.is_none());
    assert!(report.projects[&4].web_base.is_none());
    assert_eq!(report.failures[&FailureKind::Failed].0, 2);
    let expected_origin = Url::parse("https://another.example.org").unwrap().origin();
    assert_eq!(report.elsewhere, Some((2, expected_origin)));
    assert_eq!(
        warnings(&id)
            .iter()
            .filter(|line| line.message.contains("https://another.example.org"))
            .count(),
        1
    );
}

#[tokio::test]
async fn project_lookup_pays_for_each_request_and_respects_the_shared_limit() {
    let server =
        TestServer::start(|_| Reply::json(200, r#"{"id":1,"path_with_namespace":"acme/one"}"#));
    let api = client(&server);
    let budget = Budget::new(1, Instant::now() + std::time::Duration::from_secs(5));
    let projects = read_projects([1, 2].into(), &api, &budget).await;
    assert_eq!(projects.projects.len(), 1);
    assert_eq!(budget.spent(), 1);
    assert_eq!(budget.try_spend().unwrap_err().kind(), FailureKind::Limit);
    assert_eq!(server.requests()[0].path(), "/api/v4/projects/1");
    let ended = Budget::new(4, Instant::now());
    let expired = read_projects([1].into(), &api, &ended).await;
    assert!(expired.projects.is_empty());
    assert_eq!(expired.failures[&FailureKind::Limit].0, 1);
    assert_eq!(ended.spent(), 0);
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn a_busy_or_refused_project_stops_stacks_without_a_retry() {
    for (status, kind) in [
        (429, FailureKind::Throttled),
        (401, FailureKind::TokenInvalid),
    ] {
        let server =
            TestServer::start(move |_| Reply::json(status, "{}").with_header("retry-after", "0"));
        let budget = Budget::for_collection();
        let api = client(&server);
        let report = read_projects(
            [1].into(),
            &api,
            &budget.phase(PROJECT_REQUESTS, PROJECT_TIME),
        )
        .await;
        assert!(report.projects.is_empty());
        assert_eq!(report.failures[&kind].0, 1);
        assert_eq!(budget.check().unwrap_err().kind(), kind);
        let change = ChangeRef {
            repo: 1,
            number: 12,
        };
        assert!(
            stack::look_up(
                vec![(change, Timestamp::from("2026-03-05T09:00:00Z").utc())],
                &api,
                &budget
            )
            .await
            .is_empty()
        );
        assert_eq!(server.requests().len(), 1);
    }
}

fn warnings(id: &ConnectionId) -> Vec<crate::model::LogEntry> {
    diagnostics::entries()
        .into_iter()
        .filter(|line| line.scope == id.to_string() && line.level == LogLevel::Warn)
        .collect()
}

#[tokio::test]
async fn project_failures_are_counted_in_one_warning() {
    let server = TestServer::start(|request| match request.path() {
        "/api/v4/projects/1" => Reply::json(403, r#"{"message":"Project hidden"}"#),
        "/api/v4/projects/2" => Reply::json(404, "{}"),
        _ => panic!("unexpected project lookup"),
    });
    let id = ConnectionId::new(Source::Gitlab, "project-lookup-warning");
    let report = diagnostics::in_connection(id.clone(), async {
        let report =
            read_projects([1, 2].into(), &client(&server), &Budget::for_collection()).await;
        report.log();
        report
    })
    .await;
    assert!(report.projects.is_empty());
    assert_eq!(report.failures.len(), 1);
    let (count, cause) = &report.failures[&FailureKind::Unreadable];
    assert_eq!(*count, 2);
    assert!(matches!(
        cause,
        Some(LookupCause::Request(ApiError::Status {
            status: reqwest::StatusCode::FORBIDDEN,
            ..
        }))
    ));
    assert_eq!(warnings(&id).len(), 1);
}

fn event(project_id: u64, timestamp: impl serde::Serialize) -> GitLabEntry {
    serde_json::from_value(json!({
        "id": format!("event-{project_id}"), "source": "gitlab", "connection": "work",
        "type": "opened", "timestamp": timestamp, "projectId": project_id, "mrIid": 12,
    }))
    .unwrap()
}

#[test]
fn pushes_and_commit_notes_have_no_item_url() {
    let project = ProjectDetails {
        name: "acme/widgets".into(),
        web_base: Some(Url::parse("https://gitlab.example.org/acme/widgets").unwrap()),
    };
    let mut push = event(1, "2026-03-05T09:00:00Z");
    push.entry_type = GitLabType::PushedTo;
    let mut note = event(1, "2026-03-05T09:30:00Z");
    note.entry_type = GitLabType::CommentedOn;
    note.target_type = Some("Note".into());
    note.mr_iid = None;
    assert_eq!(project.name, "acme/widgets");
    assert_eq!(project.item_url(&push), None);
    assert_eq!(project.item_url(&note), None);
}

#[tokio::test]
async fn the_newest_twenty_projects_get_names_and_all_projects_get_stacks() {
    let server = TestServer::start(|request| {
        let project = request.path().strip_prefix("/api/v4/projects/").unwrap();
        let answer = if let Some(id) = project.strip_suffix("/merge_requests") {
            let id: u64 = id.parse().unwrap();
            let row = |iid, source, target| {
                json!({
                    "id": iid, "iid": iid, "title": format!("Change {iid}"), "state": "opened",
                    "source_project_id": id, "target_project_id": id,
                    "source_branch": source, "target_branch": target,
                })
            };
            json!([row(11, "base", "main"), row(12, "feature", "base")])
        } else {
            json!({"id":project.parse::<u64>().unwrap(), "path_with_namespace":format!("acme/project-{project}")})
        };
        Reply::json(200, answer.to_string())
    });
    let start = Timestamp::from("2026-03-05T09:00:00Z").utc();
    let mut entries: Vec<_> = (1..=25)
        .map(|id| {
            let seconds = if id == 6 { 5 } else { id as i64 };
            ActivityEntry::GitLab(event(id, start + chrono::TimeDelta::seconds(seconds)))
        })
        .collect();
    // An older event later in the input must not reorder project 25.
    entries.push(ActivityEntry::GitLab(event(25, start)));
    let (report, stacks) = look_up(&entries, &client(&server)).await;
    assert_eq!(report.requests, 20);
    let named: std::collections::BTreeSet<_> = report.projects.keys().copied().collect();
    assert_eq!(named, std::iter::once(5).chain(7..=25).collect());
    assert_eq!(report.failures.len(), 1);
    assert_eq!(report.failures[&FailureKind::Limit], (5, None));
    assert_eq!(stacks.len(), 25);
    assert_eq!(
        stacks[&ChangeRef {
            repo: 1,
            number: 12
        }]
            .below()[0]
            .number,
        11
    );
    let requests = server.requests();
    assert_eq!(requests.len(), 45);
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.path().ends_with("/merge_requests"))
            .count(),
        25
    );
}
