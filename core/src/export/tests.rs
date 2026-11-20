use chrono_tz::Europe::Berlin;

use super::{render_export, render_export_request};
use crate::model::{
    ActivityEntry, ActivityLog, ExportRequest, GitEntry, MeetingWeight, OutlookEntry, OutputFormat,
    Source,
};

fn git(timestamp: &str) -> ActivityEntry {
    let mut entry = GitEntry::new(
        "repo".to_string(),
        "abc123".to_string(),
        "Fix the build".to_string(),
        "me@example.com".to_string(),
        "main".to_string(),
        timestamp.to_string(),
    );
    entry.connection = "this-computer".to_string();
    ActivityEntry::Git(entry)
}

fn meeting(timestamp: &str) -> ActivityEntry {
    ActivityEntry::Outlook(OutlookEntry {
        source: "outlook".to_string(),
        connection: "default".to_string(),
        id: format!("planning/{timestamp}"),
        timestamp: timestamp.to_string(),
        title: "Planning".to_string(),
        location: Some("Room 1".to_string()),
        organizer: None,
        is_organizer: Some(true),
        duration_minutes: 30,
        busy_status: None,
        meeting_weight: MeetingWeight::Busy,
        description: None,
        description_blocks: Vec::new(),
        join_url: None,
    })
}

fn request(format: OutputFormat, start: &str, end: &str, time_zone: &str) -> ExportRequest {
    ExportRequest {
        format,
        start: start.to_string(),
        end: end.to_string(),
        time_zone: time_zone.to_string(),
        entries: vec![meeting("2026-10-25T14:00:00Z"), git("2026-10-24T22:30:00Z")],
    }
}

#[test]
fn names_the_file_after_the_local_days_of_the_range_across_a_clock_change() {
    // Berlin's Saturday 24 to Sunday 25 October 2026: summer time ends on Sunday, so the range
    // starts at UTC+2 and ends at UTC+1.
    let file = render_export_request(&request(
        OutputFormat::Markdown,
        "2026-10-23T22:00:00Z",
        "2026-10-25T23:00:00Z",
        "Europe/Berlin",
    ))
    .unwrap();

    assert_eq!(file.file_name, "activity-2026-10-24-2026-10-25.md");
}

#[test]
fn renders_markdown_by_day_in_the_requested_zone() {
    let file = render_export_request(&request(
        OutputFormat::Markdown,
        "2026-10-23T22:00:00Z",
        "2026-10-25T23:00:00Z",
        "Europe/Berlin",
    ))
    .unwrap();

    assert!(file.content.starts_with(
        "# Activity Summary — 2026-10-24 00:00 to 2026-10-26 00:00\n\nTotal: 2 events\n"
    ));
    // 22:30 UTC on Saturday is half past midnight on Sunday in Berlin (still summer time).
    let sunday = file.content.find("## Sunday, 25 October 2026").unwrap();
    assert!(file.content[sunday..].contains("- **[git]** `repo` Fix the build (00:30)"));
    // 14:00 UTC after the change is 15:00 winter time.
    assert!(file.content[sunday..].contains("- **[outlook]** Planning @ Room 1 (15:00, 30m)"));
    assert!(!file.content.contains("## Saturday"));
}

#[test]
fn renders_json_as_the_activity_log_with_a_summary() {
    let file = render_export_request(&request(
        OutputFormat::Json,
        "2026-10-23T22:00:00Z",
        "2026-10-25T23:00:00Z",
        "Europe/Berlin",
    ))
    .unwrap();

    assert_eq!(file.file_name, "activity-2026-10-24-2026-10-25.json");
    let log: ActivityLog = serde_json::from_str(&file.content).unwrap();
    assert_eq!(log.entries.len(), 2);
    assert_eq!(log.meta.start, "2026-10-23T22:00:00+00:00");
    assert!(log.meta.pending.is_empty());
    let counts = log
        .summary
        .connections
        .iter()
        .map(|count| (count.connection.source.clone(), count.count))
        .collect::<Vec<_>>();
    assert_eq!(counts, [(Source::Git, 1), (Source::Outlook, 1)]);
}

#[test]
fn rejects_an_unknown_zone_and_a_bad_range() {
    let zone = render_export_request(&request(
        OutputFormat::Json,
        "2026-10-23T22:00:00Z",
        "2026-10-25T23:00:00Z",
        "Mars/Olympus",
    ))
    .unwrap_err();
    assert!(
        zone.to_string()
            .contains("Unknown time zone \"Mars/Olympus\"")
    );

    let backwards = render_export_request(&request(
        OutputFormat::Json,
        "2026-10-25T23:00:00Z",
        "2026-10-23T22:00:00Z",
        "Europe/Berlin",
    ))
    .unwrap_err();
    assert!(format!("{backwards:#}").contains("end must be after start"));
}

#[test]
fn renders_a_collected_log_for_any_zone() {
    let log = ActivityLog {
        meta: crate::model::ActivityLogMeta {
            generated: "2026-10-26T00:00:00Z".to_string(),
            start: "2026-10-23T22:00:00Z".to_string(),
            end: "2026-10-25T23:00:00Z".to_string(),
            pending: Vec::new(),
        },
        summary: Default::default(),
        entries: vec![git("2026-10-24T22:30:00Z")],
    };

    let file = render_export(&log, OutputFormat::Markdown, &Berlin);
    assert_eq!(file.file_name, "activity-2026-10-24-2026-10-25.md");
    let utc = render_export(&log, OutputFormat::Markdown, &chrono::Utc);
    assert_eq!(utc.file_name, "activity-2026-10-23-2026-10-25.md");
    assert!(utc.content.contains("## Saturday, 24 October 2026"));
}
