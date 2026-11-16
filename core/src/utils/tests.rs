use serde_json::json;

use chrono::FixedOffset;

use super::{MergeResult, format_duration, merge_git_with_gitlab, to_markdown};
use crate::model::{
    ActivityEntry, ActivityLog, ActivityLogMeta, CollectedActivitySummary, GitEntry, GitLabEntry,
};

#[test]
fn merges_git_and_gitlab_push_events() {
    let git = vec![GitEntry::new(
        "repo".to_string(),
        "abc123".to_string(),
        "message".to_string(),
        "me@example.com".to_string(),
        "main".to_string(),
        "2026-03-10T10:00:00Z".to_string(),
    )];
    let gitlab = vec![GitLabEntry {
        source: "gitlab".to_string(),
        entry_type: crate::model::GitLabType::PushedTo,
        timestamp: "2026-03-10T10:05:00Z".to_string(),
        title: "repo".to_string(),
        target_type: "Project".to_string(),
        mr_iid: None,
        branch: Some("main".to_string()),
        commit_count: Some(1),
        note_body: None,
        note_file: None,
        note_resolved: None,
        raw: json!({ "push_data": { "commit_to": "abc123" } }),
    }];

    let MergeResult {
        merged_git,
        remaining_gitlab,
    } = merge_git_with_gitlab(&git, &gitlab);

    assert!(merged_git[0].gitlab_event.is_some());
    assert!(remaining_gitlab.is_empty());
}

#[test]
fn markdown_contains_expected_sections() {
    let log = ActivityLog {
        meta: ActivityLogMeta {
            generated: "2026-03-10T10:00:00Z".to_string(),
            start: "2026-03-01T00:00:00Z".to_string(),
            end: "2026-03-11T00:00:00Z".to_string(),
        },
        summary: CollectedActivitySummary::default(),
        entries: vec![ActivityEntry::Git(GitEntry::new(
            "repo".to_string(),
            "abc123".to_string(),
            "message".to_string(),
            "me@example.com".to_string(),
            "main".to_string(),
            "2026-03-10T23:30:00Z".to_string(),
        ))],
    };

    // Rendered for a reader at UTC+2: the commit lands on the 11th at 01:30.
    let markdown = to_markdown(&log, &FixedOffset::east_opt(2 * 3600).unwrap());
    assert!(markdown.contains("# Activity Summary — 2026-03-01 02:00 to 2026-03-11 02:00"));
    assert!(markdown.contains("## Wednesday, 11 March 2026"));
    assert!(markdown.contains("**[git]** `repo` message (01:30)"));
}

#[test]
fn formats_duration() {
    assert_eq!(format_duration(45), "45m");
    assert_eq!(format_duration(60), "1h");
    assert_eq!(format_duration(90), "1h 30m");
}
