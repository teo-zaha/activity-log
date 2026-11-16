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
