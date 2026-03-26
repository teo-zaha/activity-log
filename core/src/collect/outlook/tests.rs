use crate::model::outlook::raw_outlook_event::RawOutlookEvent;

use super::{normalize_outlook_events, parse_raw_outlook_events};

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
    let normalized = normalize_outlook_events(vec![RawOutlookEvent {
        subject: None,
        start: "2026-03-25T09:00:00Z".to_string(),
        end: "2026-03-25T09:45:00Z".to_string(),
        location: Some("Desk".to_string()),
        organizer: None,
        is_organizer: Some(true),
    }]);

    assert_eq!(normalized[0].title, "(No title)");
    assert_eq!(normalized[0].duration_minutes, 45);
    assert_eq!(normalized[0].location.as_deref(), Some("Desk"));
}
