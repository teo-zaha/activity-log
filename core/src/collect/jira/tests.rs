use chrono::{TimeZone, Utc};

use serde_json::json;

use super::{
    collect_jira, describe_issues, jira_search_jql, map_jira_issues, next_start, partial_changelogs,
};
use crate::{
    model::{
        CollectOptions, ConnectionId, JiraEntryType, Source,
        jira::{
            jira_changelog::JiraChangelog, jira_history::JiraHistory,
            jira_history_item::JiraHistoryItem, jira_issue::JiraIssue,
            jira_issue_fields::JiraIssueFields, jira_named_field::JiraNamedField,
            jira_search_response::JiraSearchResponse,
        },
    },
    provider::{
        connection::{Connection, ConnectionLocation, ConnectionValues},
        jira::JiraConfig,
        provider_config::ProviderConfigResolve,
        provider_context::ProviderContext,
        remote_api::test_server::{Reply, TestServer},
        remote_error::RemoteFailure,
    },
};

fn march() -> CollectOptions {
    CollectOptions {
        start: Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap(),
        end: Utc.with_ymd_and_hms(2026, 4, 1, 0, 0, 0).unwrap(),
    }
}

fn issue(created: &str, updated: &str, history: &str) -> JiraIssue {
    JiraIssue {
        key: "ABC-123".to_string(),
        fields: JiraIssueFields {
            summary: "Rewrite backend".to_string(),
            status: JiraNamedField {
                name: "In Progress".to_string(),
            },
            issuetype: JiraNamedField {
                name: "Task".to_string(),
            },
            created: created.to_string(),
            updated: updated.to_string(),
        },
        changelog: Some(JiraChangelog {
            histories: vec![JiraHistory {
                id: None,
                created: history.to_string(),
                items: vec![JiraHistoryItem {
                    field: "status".to_string(),
                    from_string: Some("To Do".to_string()),
                    to_string: Some("In Progress".to_string()),
                }],
            }],
            ..JiraChangelog::default()
        }),
    }
}

#[test]
fn maps_jira_issues() {
    let issues = vec![issue(
        "2026-03-05T08:00:00Z",
        "2026-03-05T09:00:00Z",
        "2026-03-05T09:00:00Z",
    )];

    let entries = map_jira_issues(&issues, &march());
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].key, "ABC-123");
    assert_eq!(entries[0].entry_type, JiraEntryType::IssueCreated);
    assert_eq!(entries[1].entry_type, JiraEntryType::StatusChanged);
    assert_eq!(entries[0].id, "ABC-123:created");
    assert_eq!(
        entries[1].id,
        "ABC-123:status:2026-03-05T09:00:00+00:00:In Progress"
    );
    assert_eq!(entries[1].from_status.as_deref(), Some("To Do"));
    assert_eq!(entries[1].to_status.as_deref(), Some("In Progress"));
}

#[test]
fn reads_jira_offsets_and_cuts_at_the_exact_range() {
    // Jira's own format: an offset without a colon. Created 2026-02-28T23:30Z, so before
    // March; the status change at 2026-03-01T00:30Z is inside.
    let issues = vec![issue(
        "2026-03-01T00:30:00.000+0100",
        "2026-03-01T01:30:00.000+0100",
        "2026-03-01T01:30:00.000+0100",
    )];

    let entries = map_jira_issues(&issues, &march());

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].entry_type, JiraEntryType::StatusChanged);
    assert_eq!(entries[0].timestamp, "2026-03-01T01:30:00+01:00");
}

#[test]
fn jira_search_has_no_upper_updated_bound() {
    // An issue changed inside the range and again afterwards must still be found.
    let jql = jira_search_jql("2026-10-01");
    assert_eq!(
        jql,
        "updatedDate >= \"2026-10-01\" AND (assignee = currentUser() OR reporter = currentUser())"
    );
}

#[test]
fn a_status_change_is_named_by_its_changelog_entry() {
    let mut with_id = issue(
        "2026-02-05T08:00:00Z",
        "2026-03-06T09:00:00Z",
        "2026-03-05T09:00:00Z",
    );
    with_id.changelog.as_mut().unwrap().histories[0].id = Some("10042".to_string());
    let without_change = issue(
        "2026-02-05T08:00:00Z",
        "2026-03-06T09:00:00Z",
        "2026-02-06T09:00:00Z",
    );

    let changed = map_jira_issues(&[with_id], &march());
    let updated = map_jira_issues(&[without_change], &march());

    assert_eq!(changed[0].id, "ABC-123:status:10042");
    assert_eq!(updated[0].entry_type, JiraEntryType::IssueUpdated);
    assert_eq!(updated[0].id, "ABC-123:updated:2026-03-06T09:00:00+00:00");
}

/// Collects `options` through the day cache, the issue last updated at `updated`.
async fn cached(
    path: &std::path::Path,
    options: CollectOptions,
    updated: &'static str,
) -> Vec<crate::model::ActivityEntry> {
    use std::time::Duration;

    use crate::{
        cache::{CachePolicy, collect_with_day_cache_at},
        model::{ConnectionId, IntoActivityEntries, Source},
    };

    let policy = CachePolicy {
        recent_ttl: Duration::from_secs(5 * 60),
        settled_ttl: Duration::from_secs(7 * 24 * 60 * 60),
        ..CachePolicy::api()
    };
    collect_with_day_cache_at(
        path,
        &ConnectionId::new(Source::Jira, "default"),
        &options,
        policy,
        || Utc.with_ymd_and_hms(2026, 3, 4, 12, 0, 0).unwrap(),
        |span| async move {
            let issue = issue("2026-02-01T08:00:00Z", updated, "2026-02-01T08:00:00Z");
            let mut entries = map_jira_issues(&[issue], &span).into_activity_entries();
            for entry in &mut entries {
                entry.set_connection(&"default".into());
            }
            Ok(entries)
        },
    )
    .await
    .unwrap()
}

/// An issue updated on Monday, its day cached and settled, then updated again on Wednesday: both
/// updates are entries, with ids that differ and do not depend on the range asked for.
#[tokio::test]
async fn updates_on_two_cached_days_have_ids_of_their_own() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("jira.json");
    let day = |d: u32| CollectOptions {
        start: Utc.with_ymd_and_hms(2026, 3, d, 0, 0, 0).unwrap(),
        end: Utc.with_ymd_and_hms(2026, 3, d + 1, 0, 0, 0).unwrap(),
    };

    cached(&path, day(2), "2026-03-02T09:00:00Z").await;
    let entries = cached(
        &path,
        CollectOptions {
            start: day(2).start,
            end: day(4).end,
        },
        "2026-03-04T09:00:00Z",
    )
    .await;

    let ids = entries
        .iter()
        .map(crate::model::ActivityEntry::id)
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        [
            "ABC-123:updated:2026-03-02T09:00:00+00:00",
            "ABC-123:updated:2026-03-04T09:00:00+00:00",
        ]
    );
}

const TOKEN: &str = "jira-example-token";

fn context(url: &str) -> ProviderContext {
    let connection = Connection::new(
        ConnectionId::new(Source::Jira, "work"),
        "Work Jira",
        ConnectionLocation::Local,
        ConnectionValues::new(
            [
                ("JIRA_URL".to_string(), Some(url.to_string())),
                ("JIRA_TOKEN".to_string(), Some(TOKEN.to_string())),
            ]
            .into(),
        ),
    );
    let mut context = ProviderContext::for_connection(&connection, JiraConfig::requirements());
    context.with_typed::<JiraConfig>().unwrap();
    context
}

fn issue_json(key: &str) -> serde_json::Value {
    json!({
        "key": key,
        "fields": {
            "summary": "Task",
            "status": { "name": "Open" },
            "issuetype": { "name": "Task" },
            "created": "2026-03-05T08:00:00.000+0000",
            "updated": "2026-03-05T08:00:00.000+0000"
        }
    })
}

#[test]
fn paging_follows_what_the_server_sent() {
    let page = |received, total, is_last| (received, total, is_last);
    let next = |start, (received, total, is_last)| next_start(start, received, total, is_last);
    assert_eq!(next(0, page(20, Some(50), None)), Some(20), "a capped page");
    assert_eq!(next(20, page(20, Some(50), None)), Some(40));
    assert_eq!(next(40, page(10, Some(50), None)), None, "total reached");
    assert_eq!(
        next(0, page(0, Some(50), None)),
        None,
        "an empty page ends it"
    );
    assert_eq!(next(0, page(50, None, None)), Some(50));
    assert_eq!(
        next(50, page(20, None, None)),
        Some(70),
        "a short page alone does not end it"
    );
    assert_eq!(next(70, page(0, None, None)), None);
    assert_eq!(next(0, page(20, None, Some(true))), None, "isLast ends it");
    assert_eq!(next(0, page(20, Some(50), Some(false))), Some(20));
}

#[tokio::test]
async fn a_server_that_caps_pages_still_gives_every_issue() {
    let server = TestServer::start(|request| {
        let start: usize = request.query("startAt").unwrap().parse().unwrap();
        let issues: Vec<_> = (start..(start + 20).min(50))
            .map(|n| issue_json(&format!("ABC-{n}")))
            .collect();
        Reply::json(
            200,
            json!({ "startAt": start, "maxResults": 20, "total": 50, "issues": issues })
                .to_string(),
        )
    });

    let entries = collect_jira(&march(), &context(&server.url)).await.unwrap();

    assert_eq!(entries.len(), 50);
    let requests = server.requests();
    let starts: Vec<_> = requests
        .iter()
        .map(|request| request.query("startAt").unwrap())
        .collect();
    assert_eq!(starts, ["0", "20", "40"]);
    assert_eq!(requests[0].path(), "/rest/api/2/search");
    assert_eq!(
        requests[0].query("jql").as_deref(),
        Some(jira_search_jql("2026-02-28").as_str())
    );
    assert_eq!(requests[0].query("maxResults").as_deref(), Some("50"));
    assert_eq!(requests[0].query("expand").as_deref(), Some("changelog"));
}

#[tokio::test]
async fn a_capped_server_without_a_total_still_gives_every_issue() {
    let server = TestServer::start(|request| {
        let start: usize = request.query("startAt").unwrap().parse().unwrap();
        let issues: Vec<_> = (start..(start + 20).min(50))
            .map(|n| issue_json(&format!("ABC-{n}")))
            .collect();
        Reply::json(
            200,
            json!({ "startAt": start, "issues": issues }).to_string(),
        )
    });

    let entries = collect_jira(&march(), &context(&server.url)).await.unwrap();

    assert_eq!(entries.len(), 50);
    let starts: Vec<_> = server
        .requests()
        .iter()
        .map(|request| request.query("startAt").unwrap())
        .collect();
    assert_eq!(starts, ["0", "20", "40", "50"]);
}

#[tokio::test]
async fn a_page_from_another_offset_fails_the_collection() {
    // Always the first page, whatever was asked: reported as `startAt: 0`, or not at all.
    for report_start in [true, false] {
        let server = TestServer::start(move |_| {
            let issues: Vec<_> = (0..20).map(|n| issue_json(&format!("ABC-{n}"))).collect();
            let mut page = json!({ "issues": issues });
            if report_start {
                page["startAt"] = json!(0);
            }
            Reply::json(200, page.to_string())
        });

        let error = collect_jira(&march(), &context(&server.url))
            .await
            .unwrap_err();

        assert!(
            error.is::<super::PagesOutOfStep>(),
            "startAt reported: {report_start}: {error:#}"
        );
        assert!(error.downcast_ref::<RemoteFailure>().is_none());
        assert_eq!(server.requests().len(), 2);
    }
}

#[test]
fn partial_changelogs_are_detected() {
    let page: JiraSearchResponse = serde_json::from_value(json!({
        "total": 2,
        "issues": [
            {
                "key": "PART-7",
                "fields": issue_json("PART-7")["fields"],
                "changelog": { "startAt": 0, "maxResults": 1, "total": 3, "histories": [
                    { "id": "1", "created": "2026-03-05T09:00:00.000+0000", "items": [] }
                ] }
            },
            {
                "key": "FULL-1",
                "fields": issue_json("FULL-1")["fields"],
                "changelog": { "startAt": 0, "maxResults": 1, "total": 1, "histories": [
                    { "id": "2", "created": "2026-03-05T09:00:00.000+0000", "items": [] }
                ] }
            }
        ]
    }))
    .unwrap();

    assert_eq!(partial_changelogs(&page.issues), ["PART-7"]);
}

#[test]
fn the_partial_changelog_warning_shows_only_well_formed_keys() {
    let keys = |keys: &[&str]| keys.iter().map(|key| key.to_string()).collect::<Vec<_>>();
    assert_eq!(describe_issues(&keys(&["ABC-1"])), "ABC-1");
    assert_eq!(
        describe_issues(&keys(&[
            "ABC-1",
            "EVIL\u{1b}[2J-1",
            "abc-2",
            "ABC-01",
            &"A".repeat(200),
            "B_2-9",
        ])),
        "ABC-1, B_2-9 and 4 more"
    );
    assert_eq!(
        describe_issues(&keys(&["A1-1", "A2-1", "A3-1", "A4-1", "A5-1", "A6-1"])),
        "A1-1, A2-1, A3-1, A4-1, A5-1 and 1 more"
    );
    assert_eq!(describe_issues(&keys(&["\u{202e}x", "y"])), "2 issues");
}

#[tokio::test]
async fn a_partial_changelog_is_reported() {
    let server = TestServer::start(|_| {
        let mut issue = issue_json("TRUNC-31");
        issue["changelog"] =
            json!({ "startAt": 0, "maxResults": 100, "total": 140, "histories": [] });
        Reply::json(200, json!({ "total": 1, "issues": [issue] }).to_string())
    });

    collect_jira(&march(), &context(&server.url)).await.unwrap();

    assert!(crate::diagnostics::entries().iter().any(|entry| {
        entry.message
            == "Jira sent only part of the change history of TRUNC-31; status changes outside it are missing"
    }));
}

#[tokio::test]
async fn collector_errors_are_short_and_clean() {
    let server = TestServer::start(|_| {
        Reply::json(
            400,
            format!(r#"{{"errorMessages":["The value '{TOKEN}' does not exist"],"stack":"x"}}"#),
        )
    });

    let error = collect_jira(&march(), &context(&format!("{}/jira", server.url)))
        .await
        .unwrap_err()
        .to_string();

    assert_eq!(
        error,
        "Jira answered 400 Bad Request: The value '[redacted]' does not exist"
    );
}

/// A status change on Monday and a later edit on Tuesday: the edit shows however the range is
/// split, and a status change that is the latest change is not shown twice.
#[test]
fn a_later_edit_shows_beside_an_earlier_status_change_in_any_partition() {
    let monday_status_tuesday_edit = issue(
        "2026-02-05T08:00:00Z",
        "2026-03-03T10:00:00Z",
        "2026-03-02T09:00:00Z",
    );
    let day = |day: u32| CollectOptions {
        start: Utc.with_ymd_and_hms(2026, 3, day, 0, 0, 0).unwrap(),
        end: Utc.with_ymd_and_hms(2026, 3, day + 1, 0, 0, 0).unwrap(),
    };
    let kinds = |options: &CollectOptions| {
        map_jira_issues(std::slice::from_ref(&monday_status_tuesday_edit), options)
            .into_iter()
            .map(|entry| (entry.entry_type, entry.timestamp.to_string()))
            .collect::<Vec<_>>()
    };

    let both_days = CollectOptions {
        start: day(2).start,
        end: day(3).end,
    };
    let together = kinds(&both_days);
    let mut apart = kinds(&day(2));
    apart.extend(kinds(&day(3)));
    assert_eq!(together, apart);
    assert_eq!(
        together,
        [
            (
                JiraEntryType::StatusChanged,
                "2026-03-02T09:00:00+00:00".to_string()
            ),
            (
                JiraEntryType::IssueUpdated,
                "2026-03-03T10:00:00+00:00".to_string()
            ),
        ]
    );

    // Jira's `updated` is the status change itself, to the millisecond or within its second.
    for history in ["2026-03-03T10:00:00Z", "2026-03-03T10:00:00.400+0000"] {
        let status_is_latest = issue(
            "2026-02-05T08:00:00Z",
            "2026-03-03T10:00:00.000+0000",
            history,
        );
        let entries = map_jira_issues(&[status_is_latest], &march());
        assert_eq!(entries.len(), 1, "{history}");
        assert_eq!(entries[0].entry_type, JiraEntryType::StatusChanged);
    }
}

/// A status change just before midnight and another edit just after it: the edit is the
/// latest change and shows on its own day.
#[test]
fn an_edit_right_after_a_status_change_is_its_own_entry() {
    let mut issue = issue(
        "2026-02-05T08:00:00Z",
        "2026-03-03T00:00:00.100+0000",
        "2026-03-02T23:59:59.900+0000",
    );
    issue
        .changelog
        .as_mut()
        .unwrap()
        .histories
        .push(JiraHistory {
            id: Some("10043".to_string()),
            created: "2026-03-03T00:00:00.100+0000".to_string(),
            items: vec![JiraHistoryItem {
                field: "summary".to_string(),
                from_string: None,
                to_string: None,
            }],
        });
    let tuesday = CollectOptions {
        start: Utc.with_ymd_and_hms(2026, 3, 3, 0, 0, 0).unwrap(),
        end: Utc.with_ymd_and_hms(2026, 3, 4, 0, 0, 0).unwrap(),
    };

    let entries = map_jira_issues(&[issue], &tuesday);

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].entry_type, JiraEntryType::IssueUpdated);
}
