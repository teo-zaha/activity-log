use chrono::{DateTime, Days, Duration, NaiveDate, Utc};

use crate::model::{
    BusyStatus, CollectOptions, MeetingWeight, outlook::raw_outlook_event::RawOutlookEvent,
};

use super::{
    OutlookFallback, busy_status_from_com, normalize_outlook_events, parse_raw_outlook_events,
};

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
            id: None,
            subject: Some("Bad dates".to_string()),
            start: "not-a-date".to_string(),
            end: "also-not-a-date".to_string(),
            location: None,
            organizer: None,
            is_organizer: Some(false),
            is_default_calendar: Some(true),
            busy_status: None,
            body: None,
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
            id: None,
            subject: None,
            start: "2026-03-25T09:00:00Z".to_string(),
            end: "2026-03-25T09:45:00Z".to_string(),
            location: Some("Desk".to_string()),
            organizer: None,
            is_organizer: Some(true),
            is_default_calendar: Some(true),
            busy_status: None,
            body: None,
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
        id: None,
        subject: Some(subject.to_string()),
        start: from.to_rfc3339(),
        end: to.to_rfc3339(),
        location: None,
        organizer: None,
        is_organizer: Some(false),
        is_default_calendar: Some(true),
        busy_status: None,
        body: None,
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
                id: None,
                subject: Some("Default".to_string()),
                start: "2026-03-25T09:00:00Z".to_string(),
                end: "2026-03-25T09:30:00Z".to_string(),
                location: None,
                organizer: None,
                is_organizer: Some(false),
                is_default_calendar: Some(true),
                busy_status: None,
                body: None,
            },
            RawOutlookEvent {
                id: None,
                subject: Some("Shared".to_string()),
                start: "2026-03-25T10:00:00Z".to_string(),
                end: "2026-03-25T10:30:00Z".to_string(),
                location: None,
                organizer: None,
                is_organizer: Some(false),
                is_default_calendar: Some(false),
                busy_status: None,
                body: None,
            },
        ],
        &options("2026-03-25", "2026-03-25"),
    );

    assert_eq!(normalized.len(), 1);
    assert_eq!(normalized[0].title, "Default");
}

#[test]
fn running_classic_outlook_is_connected_even_without_a_registered_object() {
    let processes = ["System", "explorer.exe", "OUTLOOK.EXE"];

    assert_eq!(
        OutlookFallback::decide(processes, false),
        OutlookFallback::Connect
    );
    assert_eq!(
        OutlookFallback::decide(processes, true),
        OutlookFallback::Elevated
    );
}

#[test]
fn classic_outlook_wins_over_the_new_one() {
    assert_eq!(
        OutlookFallback::decide(["olk.exe", "outlook.exe"], false),
        OutlookFallback::Connect
    );
}

#[test]
fn reports_new_outlook_and_a_closed_outlook() {
    assert_eq!(
        OutlookFallback::decide(["explorer.exe", "olk.exe"], false),
        OutlookFallback::NewOutlookOnly
    );
    assert_eq!(
        OutlookFallback::decide(["explorer.exe"], true),
        OutlookFallback::NotRunning
    );
    assert!(
        OutlookFallback::NewOutlookOnly
            .message()
            .contains("classic Outlook")
    );
}

#[test]
fn maps_every_com_busy_status() {
    assert_eq!(busy_status_from_com(0), Some(BusyStatus::Free));
    assert_eq!(busy_status_from_com(1), Some(BusyStatus::Tentative));
    assert_eq!(busy_status_from_com(2), Some(BusyStatus::Busy));
    assert_eq!(busy_status_from_com(3), Some(BusyStatus::OutOfOffice));
    assert_eq!(busy_status_from_com(4), Some(BusyStatus::WorkingElsewhere));
    assert_eq!(busy_status_from_com(5), None);
    assert_eq!(busy_status_from_com(-1), None);
}

#[test]
fn keeps_the_busy_status_and_leaves_it_out_of_json_when_unknown() {
    let event = |busy_status| RawOutlookEvent {
        id: None,
        subject: Some("Sync".to_string()),
        start: "2026-03-25T09:00:00Z".to_string(),
        end: "2026-03-25T09:30:00Z".to_string(),
        location: None,
        organizer: None,
        is_organizer: Some(false),
        is_default_calendar: Some(true),
        busy_status,
        body: None,
    };
    let normalized = normalize_outlook_events(
        vec![event(Some(BusyStatus::OutOfOffice)), event(None)],
        &options("2026-03-25", "2026-03-25"),
    );

    let json = normalized
        .iter()
        .map(|entry| serde_json::to_value(entry).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(json[0]["busyStatus"], "out_of_office");
    assert!(json[1].get("busyStatus").is_none());
    assert_eq!(json[0]["meetingWeight"], "none");
    assert_eq!(json[1]["meetingWeight"], "busy");
}

#[test]
fn weighs_meeting_time_by_busy_status() {
    let cases = [
        (None, MeetingWeight::Busy),
        (Some(BusyStatus::Busy), MeetingWeight::Busy),
        (Some(BusyStatus::WorkingElsewhere), MeetingWeight::Busy),
        (Some(BusyStatus::Tentative), MeetingWeight::Tentative),
        (Some(BusyStatus::Free), MeetingWeight::None),
        (Some(BusyStatus::OutOfOffice), MeetingWeight::None),
    ];
    for (status, weight) in cases {
        assert_eq!(MeetingWeight::of(status), weight, "{status:?}");
    }
}

#[test]
fn cleans_the_body_into_a_description_and_a_join_link() {
    let rule = "_".repeat(80);
    let body = format!(
        "Quarterly planning.\r\n\r\nSlides: https://eur03.safelinks.protection.outlook.com/ap/t-1234abcd/?url=https%3A%2F%2Fexample.org%2Fslides&data=05%7C02&reserved=0\r\n\r\n{rule}\r\nMicrosoft Teams meeting\r\nJoin: https://teams.microsoft.com/meet/123456789?p=abc\r\nMeeting ID: 123 456 789\r\n{rule}\r\n"
    );
    let event = |body: Option<String>| RawOutlookEvent {
        id: None,
        subject: Some("Planning".to_string()),
        start: "2026-03-25T09:00:00Z".to_string(),
        end: "2026-03-25T10:00:00Z".to_string(),
        location: None,
        organizer: None,
        is_organizer: Some(false),
        is_default_calendar: Some(true),
        busy_status: None,
        body,
    };
    let normalized = normalize_outlook_events(
        vec![event(Some(body)), event(None)],
        &options("2026-03-25", "2026-03-25"),
    );

    assert_eq!(
        normalized[0].description.as_deref(),
        Some("Quarterly planning.\n\nSlides: https://example.org/slides")
    );
    assert_eq!(
        normalized[0].join_url.as_deref(),
        Some("https://teams.microsoft.com/meet/123456789?p=abc")
    );
    let json = serde_json::to_value(&normalized[0]).unwrap();
    assert_eq!(
        json["joinUrl"],
        "https://teams.microsoft.com/meet/123456789?p=abc"
    );
    let json = serde_json::to_value(&normalized[1]).unwrap();
    assert!(json.get("description").is_none() && json.get("joinUrl").is_none());
}

#[test]
fn reads_an_optional_body_from_the_script_payload() {
    let events = parse_raw_outlook_events(
        r#"[{"subject":"A","start":"2026-03-25T09:00:00Z","end":"2026-03-25T09:30:00Z","body":"Agenda"},{"subject":"B","start":"2026-03-25T10:00:00Z","end":"2026-03-25T10:30:00Z"}]"#,
    )
    .unwrap();

    assert_eq!(events[0].body.as_deref(), Some("Agenda"));
    assert_eq!(events[1].body, None);
}

#[test]
fn names_an_occurrence_by_outlooks_id_and_its_start() {
    let event = |id: Option<&str>, start: &str| RawOutlookEvent {
        id: id.map(str::to_string),
        subject: Some("Standup".to_string()),
        start: start.to_string(),
        end: "2026-03-26T09:15:00Z".to_string(),
        location: None,
        organizer: Some("Ann".to_string()),
        is_organizer: Some(false),
        is_default_calendar: Some(true),
        busy_status: None,
        body: None,
    };
    let normalized = normalize_outlook_events(
        vec![
            event(Some("040000008200E0"), "2026-03-25T09:00:00Z"),
            event(Some("040000008200E0"), "2026-03-26T09:00:00Z"),
            event(None, "2026-03-25T09:00:00Z"),
        ],
        &options("2026-03-25", "2026-03-26"),
    );

    assert_eq!(normalized[0].id, "040000008200E0/2026-03-25T09:00:00Z");
    assert_eq!(normalized[1].id, "040000008200E0/2026-03-26T09:00:00Z");
    assert!(normalized[2].id.starts_with("h:"));
}
