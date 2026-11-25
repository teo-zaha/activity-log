use chrono::FixedOffset;

use super::{format_duration, to_markdown};
use crate::model::{
    ActivityEntry, ActivityLog, ActivityLogMeta, CollectedActivitySummary, GitEntry,
};

#[test]
fn markdown_contains_expected_sections() {
    let log = ActivityLog {
        meta: ActivityLogMeta {
            generated: "2026-03-10T10:00:00Z".to_string(),
            start: "2026-03-01T00:00:00Z".to_string(),
            end: "2026-03-11T00:00:00Z".to_string(),
            pending: Vec::new(),
        },
        summary: CollectedActivitySummary::default(),
        entries: vec![ActivityEntry::Git(GitEntry::new(
            "repo".to_string(),
            "abc123".to_string(),
            "message".to_string(),
            "me@example.com".to_string(),
            Some("main".to_string()),
            "2026-03-10T23:30:00Z".into(),
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

fn log_of(entries: Vec<ActivityEntry>) -> ActivityLog {
    ActivityLog {
        meta: ActivityLogMeta {
            generated: "2026-03-10T10:00:00Z".to_string(),
            start: "2026-03-10T00:00:00Z".to_string(),
            end: "2026-03-11T00:00:00Z".to_string(),
            pending: Vec::new(),
        },
        summary: CollectedActivitySummary::default(),
        entries,
    }
}

#[test]
fn literal_text_cannot_change_the_document_structure() {
    let log = log_of(vec![ActivityEntry::Git(GitEntry::new(
        "odd`repo".to_string(),
        "abc123".to_string(),
        "Fix *all* the [links](https://example.com)\n## Not a heading\n- not an item <b>x</b>"
            .to_string(),
        "me@example.com".to_string(),
        Some("main".to_string()),
        "2026-03-10T09:00:00Z".into(),
    ))]);

    let markdown = to_markdown(&log, &FixedOffset::east_opt(0).unwrap());

    let lines = markdown.lines().collect::<Vec<_>>();
    let entry = lines
        .iter()
        .find(|line| line.starts_with("- **[git]**"))
        .expect("the commit is one line");
    assert_eq!(
        *entry,
        "- **[git]** ``odd`repo`` Fix \\*all\\* the \\[links\\](https://example.com) ## Not a \
         heading - not an item \\<b\\>x\\</b\\> (09:00)"
    );
    assert!(!lines.iter().any(|line| line.starts_with("## Not")));
}

#[test]
fn code_spans_hold_any_backticks() {
    assert_eq!(super::code("repo"), "`repo`");
    assert_eq!(super::code("a`b"), "``a`b``");
    assert_eq!(super::code("`edge`"), "`` `edge` ``");
    assert_eq!(super::code("two\nlines"), "`two lines`");
    // CommonMark takes one space off each side of a span that has them both.
    assert_eq!(super::code(" spaced "), "`  spaced  `");
    assert_eq!(super::code("   "), "`   `");
    // Only ASCII spaces count: a no-break space between them is content.
    assert_eq!(super::code(" \u{a0} "), "`  \u{a0}  `");
    // An empty span would show its backticks.
    assert_eq!(super::code(""), "");
}

/// A meeting without a subject has no title in its entry; the export names it a placeholder.
#[test]
fn a_meeting_without_a_title_shows_the_placeholder() {
    let meeting = serde_json::from_str::<ActivityEntry>(
        r#"{"source":"calendar","connection":"team","id":"x","timestamp":"2026-03-10T09:00:00Z","durationMinutes":30,"meetingWeight":"busy"}"#,
    )
    .unwrap();

    let markdown = to_markdown(&log_of(vec![meeting]), &chrono::Utc);

    assert!(
        markdown.contains("- **[calendar]** (No title) (09:00, 30m)"),
        "{markdown}"
    );
}

#[test]
fn github_review_export_uses_repository_and_escapes_the_title() {
    let entry = serde_json::from_value(serde_json::json!({
        "source": "github",
        "connection": "work",
        "id": "PRR_example",
        "timestamp": "2026-03-10T09:00:00Z",
        "repository": "acme/widgets",
        "type": "reviewed",
        "pullRequest": {"number": 12, "title": "Fix *widgets*\n## Example"},
        "state": "changesRequested",
    }))
    .unwrap();
    let markdown = to_markdown(&log_of(vec![entry]), &FixedOffset::east_opt(0).unwrap());

    assert!(markdown.contains(
        "- **[github]** `acme/widgets` Requested changes on pull request #12 Fix \\*widgets\\* ## Example (09:00)\n"
    ));
    assert!(!markdown.lines().any(|line| line == "## Example"));
}
#[test]
fn github_commit_export_includes_the_branch_and_short_sha() {
    let entry = serde_json::from_value(serde_json::json!({
        "source": "github",
        "connection": "work",
        "id": "C_example",
        "timestamp": "2026-03-10T09:00:00Z",
        "repository": "acme/widgets",
        "type": "committed",
        "sha": "0123456789abcdef0123456789abcdef01234567",
        "message": "Fix *widgets*",
        "branch": "main",
    }))
    .unwrap();
    let markdown = to_markdown(&log_of(vec![entry]), &FixedOffset::east_opt(0).unwrap());

    assert!(markdown.contains(
        "- **[github]** `acme/widgets` Committed to `main` `0123456` Fix \\*widgets\\* (09:00)\n"
    ));
}

#[test]
fn figma_markdown_escapes_the_file_name_and_text_of_each_activity() {
    use crate::model::{ConnectionName, FigmaActivity, FigmaEntry, FigmaTag};
    let entry = |activity| {
        ActivityEntry::Figma(FigmaEntry {
            source: FigmaTag,
            connection: ConnectionName::unstamped(),
            id: "entry".into(),
            timestamp: "2026-03-10T09:00:00Z".into(),
            file_key: "F1".into(),
            file_name: "[Checkout]".into(),
            editor: None,
            file_url: "https://www.figma.com/file/F1".into(),
            activity,
        })
    };
    let log = log_of(vec![
        entry(FigmaActivity::VersionSaved {
            version_id: "7".into(),
            label: "*Final*".into(),
            description: None,
        }),
        entry(FigmaActivity::Edited {
            version_id: "8".into(),
        }),
        entry(FigmaActivity::Commented {
            comment_id: "9".into(),
            message: Some("One\n## Two".into()),
            node_id: None,
            resolved: false,
        }),
        entry(FigmaActivity::Replied {
            comment_id: "10".into(),
            parent_id: "9".into(),
            message: None,
            node_id: None,
        }),
    ]);

    let markdown = to_markdown(&log, &FixedOffset::east_opt(0).unwrap());
    let entries: Vec<_> = markdown
        .lines()
        .filter(|line| line.starts_with("- **[figma]**"))
        .collect();
    assert_eq!(
        entries,
        [
            r#"- **[figma]** \[Checkout\]: saved version "\*Final\*" (09:00)"#,
            r"- **[figma]** \[Checkout\]: edited (09:00)",
            r"- **[figma]** \[Checkout\]: commented: One ## Two (09:00)",
            r"- **[figma]** \[Checkout\]: replied: (No comment text) (09:00)",
        ]
    );
}
