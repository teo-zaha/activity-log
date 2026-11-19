use chrono::{TimeZone, Utc};

use super::{jira_search_jql, map_jira_issues};
use crate::model::{
    CollectOptions, JiraEntryType,
    jira::{
        jira_changelog::JiraChangelog, jira_history::JiraHistory,
        jira_history_item::JiraHistoryItem, jira_issue::JiraIssue,
        jira_issue_fields::JiraIssueFields, jira_named_field::JiraNamedField,
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
    };
    collect_with_day_cache_at(
        path,
        &ConnectionId::new(Source::Jira, "default"),
        &options,
        policy,
        Utc.with_ymd_and_hms(2026, 3, 4, 12, 0, 0).unwrap(),
        |span| async move {
            let issue = issue("2026-02-01T08:00:00Z", updated, "2026-02-01T08:00:00Z");
            Ok(map_jira_issues(&[issue], &span).into_activity_entries())
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
