use chrono::NaiveDate;

use crate::model::{CollectOptions, outlook::raw_outlook_event::RawOutlookEvent};

use super::{normalize_outlook_events, parse_raw_outlook_events};

fn options(since: &str, until: &str) -> CollectOptions {
    CollectOptions {
        since: NaiveDate::parse_from_str(since, "%Y-%m-%d").unwrap(),
        until: NaiveDate::parse_from_str(until, "%Y-%m-%d").unwrap(),
    }
}

#[test]
fn parses_outlook_event_arrays() {
    let events = parse_raw_outlook_events(
        r#"[{"subject":"Planning","start":"2026-03-25T08:00:00Z","end":"2026-03-25T08:30:00Z","location":"Room 1","organizer":"Alex","is_organizer":true}]"#,
    )
    .unwrap();

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].subject.as_deref(), Some("Planning"));
}

#[test]
fn parses_outlook_single_object_payloads() {
    let events = parse_raw_outlook_events(
        r#"{"subject":"1:1","start":"2026-03-25T09:00:00Z","end":"2026-03-25T09:45:00Z","location":null,"organizer":"Taylor","is_organizer":false}"#,
    )
    .unwrap();

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].organizer.as_deref(), Some("Taylor"));
}

#[test]
fn normalizes_outlook_duration_and_missing_subject() {
    let normalized = normalize_outlook_events(
        vec![RawOutlookEvent {
            subject: None,
            start: "2026-03-25T09:00:00Z".to_string(),
            end: "2026-03-25T09:45:00Z".to_string(),
            location: Some("Desk".to_string()),
            organizer: None,
            is_organizer: Some(true),
            is_default_calendar: Some(true),
        }],
        &options("2026-03-25", "2026-03-25"),
    );

    assert_eq!(normalized[0].title, "(No title)");
    assert_eq!(normalized[0].duration_minutes, 45);
    assert_eq!(normalized[0].location.as_deref(), Some("Desk"));
}

#[test]
fn includes_events_that_overlap_requested_range() {
    let normalized = normalize_outlook_events(
        vec![
            RawOutlookEvent {
                subject: Some("Starts before".to_string()),
                start: "2026-03-24T22:30:00Z".to_string(),
                end: "2026-03-24T23:30:00Z".to_string(),
                location: None,
                organizer: None,
                is_organizer: Some(false),
                is_default_calendar: Some(true),
            },
            RawOutlookEvent {
                subject: Some("Ends after".to_string()),
                start: "2026-03-25T22:30:00Z".to_string(),
                end: "2026-03-25T23:30:00Z".to_string(),
                location: None,
                organizer: None,
                is_organizer: Some(false),
                is_default_calendar: Some(true),
            },
            RawOutlookEvent {
                subject: Some("Outside".to_string()),
                start: "2026-03-26T01:00:00Z".to_string(),
                end: "2026-03-26T02:00:00Z".to_string(),
                location: None,
                organizer: None,
                is_organizer: Some(false),
                is_default_calendar: Some(true),
            },
        ],
        &options("2026-03-25", "2026-03-25"),
    );

    let titles = normalized
        .iter()
        .map(|entry| entry.title.as_str())
        .collect::<Vec<_>>();
    assert_eq!(titles, vec!["Starts before", "Ends after"]);
}

#[test]
fn excludes_non_default_calendar_events() {
    let normalized = normalize_outlook_events(
        vec![
            RawOutlookEvent {
                subject: Some("Default".to_string()),
                start: "2026-03-25T09:00:00Z".to_string(),
                end: "2026-03-25T09:30:00Z".to_string(),
                location: None,
                organizer: None,
                is_organizer: Some(false),
                is_default_calendar: Some(true),
            },
            RawOutlookEvent {
                subject: Some("Shared".to_string()),
                start: "2026-03-25T10:00:00Z".to_string(),
                end: "2026-03-25T10:30:00Z".to_string(),
                location: None,
                organizer: None,
                is_organizer: Some(false),
                is_default_calendar: Some(false),
            },
        ],
        &options("2026-03-25", "2026-03-25"),
    );

    assert_eq!(normalized.len(), 1);
    assert_eq!(normalized[0].title, "Default");
}
