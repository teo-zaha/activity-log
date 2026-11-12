use super::{jira_search_jql, map_jira_issues};
use crate::model::{
    JiraEntryType,
    jira::{
        jira_changelog::JiraChangelog, jira_history::JiraHistory,
        jira_history_item::JiraHistoryItem, jira_issue::JiraIssue,
        jira_issue_fields::JiraIssueFields, jira_named_field::JiraNamedField,
    },
};

#[test]
fn maps_jira_issues() {
    let issues = vec![JiraIssue {
        key: "ABC-123".to_string(),
        fields: JiraIssueFields {
            summary: "Rewrite backend".to_string(),
            status: JiraNamedField {
                name: "In Progress".to_string(),
            },
            issuetype: JiraNamedField {
                name: "Task".to_string(),
            },
            created: "2026-03-05T08:00:00Z".to_string(),
            updated: "2026-03-05T09:00:00Z".to_string(),
        },
        changelog: Some(JiraChangelog {
            histories: vec![JiraHistory {
                created: "2026-03-05T09:00:00Z".to_string(),
                items: vec![JiraHistoryItem {
                    field: "status".to_string(),
                    from_string: Some("To Do".to_string()),
                    to_string: Some("In Progress".to_string()),
                }],
            }],
        }),
    }];

    let entries = map_jira_issues(&issues, "2026-03-01", "2026-03-31");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].key, "ABC-123");
    assert_eq!(entries[0].entry_type, JiraEntryType::IssueCreated);
    assert_eq!(entries[1].entry_type, JiraEntryType::StatusChanged);
    assert_eq!(entries[1].from_status.as_deref(), Some("To Do"));
    assert_eq!(entries[1].to_status.as_deref(), Some("In Progress"));
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
