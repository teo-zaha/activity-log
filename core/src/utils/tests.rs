use serde_json::json;

use super::{MergeResult, format_duration, merge_git_with_gitlab, resolve_range, to_markdown};
use crate::model::{ActivityEntry, ActivityLog, ActivityLogMeta, GitEntry, GitLabEntry, RangePreset};

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
            since: "2026-03-01".to_string(),
            until: "2026-03-10".to_string(),
        },
        entries: vec![ActivityEntry::Git(GitEntry::new(
            "repo".to_string(),
            "abc123".to_string(),
            "message".to_string(),
            "me@example.com".to_string(),
            "main".to_string(),
            "2026-03-10T10:00:00Z".to_string(),
        ))],
    };

    let markdown = to_markdown(&log);
    assert!(markdown.contains("# Activity Summary"));
    assert!(markdown.contains("**[git]** `repo` message"));
}

#[test]
fn formats_duration() {
    assert_eq!(format_duration(45), "45m");
    assert_eq!(format_duration(60), "1h");
    assert_eq!(format_duration(90), "1h 30m");
}

#[test]
fn resolves_range_variants() {
    let (since, until) = resolve_range(RangePreset::Today);
    assert_eq!(since.len(), 10);
    assert_eq!(until.len(), 10);
}
