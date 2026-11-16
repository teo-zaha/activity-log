use chrono::{DateTime, Days, Duration, NaiveDate, Utc};

use crate::model::{CollectOptions, outlook::raw_outlook_event::RawOutlookEvent};

use super::{normalize_outlook_events, parse_raw_outlook_events};

/// Whole UTC days `since..=until`.
fn options(since: &str, until: &str) -> CollectOptions {
    let day = |value: &str| NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap();
    CollectOptions {
        start: day(since).and_hms_opt(0, 0, 0).unwrap().and_utc(),
        end: (day(until) + Days::new(1))
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc(),
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
fn parses_empty_outlook_payloads_as_no_events() {
    assert!(parse_raw_outlook_events("").unwrap().is_empty());
    assert!(parse_raw_outlook_events("   ").unwrap().is_empty());
    assert!(parse_raw_outlook_events("[]").unwrap().is_empty());
}

#[test]
fn errors_on_malformed_outlook_json() {
    assert!(parse_raw_outlook_events(r#"{"subject":"broken""#).is_err());
}

#[test]
fn excludes_outlook_events_with_unparseable_dates() {
    let normalized = normalize_outlook_events(
        vec![RawOutlookEvent {
            subject: Some("Bad dates".to_string()),
            start: "not-a-date".to_string(),
            end: "also-not-a-date".to_string(),
            location: None,
            organizer: None,
            is_organizer: Some(false),
            is_default_calendar: Some(true),
        }],
        &options("2026-03-25", "2026-03-25"),
    );

    assert!(normalized.is_empty());
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
    let options = options("2026-03-25", "2026-03-25");
    let (start, end) = (options.start, options.end);
    let event = |subject: &str, from: DateTime<Utc>, to: DateTime<Utc>| RawOutlookEvent {
        subject: Some(subject.to_string()),
        start: from.to_rfc3339(),
        end: to.to_rfc3339(),
        location: None,
        organizer: None,
        is_organizer: Some(false),
        is_default_calendar: Some(true),
    };
    let half_hour = Duration::minutes(30);

    let normalized = normalize_outlook_events(
        vec![
            event("Starts before", start - half_hour, start + half_hour),
            event("Ends after", end - half_hour, end + half_hour),
            event("Ends at the start", start - half_hour, start),
            event(
                "Outside",
                end + Duration::hours(1),
                end + Duration::hours(2),
            ),
        ],
        &options,
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
