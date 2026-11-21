use chrono::{DateTime, Days, Duration, NaiveDate, Utc};

use crate::model::{
    BusyStatus, CollectOptions, MeetingWeight, outlook::raw_outlook_event::RawOutlookEvent,
};

use super::{
    OutlookFallback, busy_status_from_com, busy_status_from_mac, is_organizer_from_com,
    normalize::normalize_outlook_events, parse_outlook_columns,
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

/// An empty subject is no title: the UI and the export show their placeholder for it.
#[test]
fn an_empty_subject_is_no_title() {
    let event = RawOutlookEvent {
        id: None,
        subject: Some("  ".to_string()),
        start: "2026-03-25T09:00:00Z".to_string(),
        end: "2026-03-25T09:45:00Z".to_string(),
        location: None,
        organizer: None,
        is_organizer: None,
        is_default_calendar: Some(true),
        busy_status: None,
        body: None,
    };
    let normalized = normalize_outlook_events(vec![event], &options("2026-03-25", "2026-03-25"));
    assert_eq!(normalized[0].title, None);
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

    assert_eq!(normalized[0].title, None);
    assert_eq!(normalized[0].meeting.duration_minutes, 45);
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
        .map(|entry| entry.title.as_deref().unwrap())
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
    assert_eq!(normalized[0].title.as_deref(), Some("Default"));
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

/// Each state Outlook can be found in names its own problem; only a classic Outlook that runs
/// is tried.
#[test]
fn an_outlook_that_cannot_be_read_says_why() {
    use crate::provider::source_failure::SourceProblem;
    assert_eq!(OutlookFallback::Connect.problem(), None);
    assert_eq!(
        OutlookFallback::Elevated.problem(),
        Some(SourceProblem::PermissionDenied)
    );
    assert_eq!(
        OutlookFallback::NewOutlookOnly.problem(),
        Some(SourceProblem::NotInstalled)
    );
    assert_eq!(
        OutlookFallback::NotRunning.problem(),
        Some(SourceProblem::NotRunning)
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
        .map(|entry| serde_json::to_value(stamped(entry)).unwrap())
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
        normalized[0].meeting.description.as_deref(),
        Some("Quarterly planning.\n\nSlides: https://example.org/slides")
    );
    assert_eq!(
        normalized[0].meeting.join_url.as_deref(),
        Some("https://teams.microsoft.com/meet/123456789?p=abc")
    );
    let json = serde_json::to_value(stamped(&normalized[0])).unwrap();
    assert_eq!(
        json["joinUrl"],
        "https://teams.microsoft.com/meet/123456789?p=abc"
    );
    let json = serde_json::to_value(stamped(&normalized[1])).unwrap();
    assert!(json.get("description").is_none() && json.get("joinUrl").is_none());
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

fn parse_outlook_columns_events(
    stdout: &str,
) -> anyhow::Result<Vec<crate::model::outlook::raw_outlook_event::RawOutlookEvent>> {
    parse_outlook_columns(stdout).map(|(events, _)| events)
}

#[test]
fn says_which_properties_outlook_would_not_give() {
    let (_, unread) = parse_outlook_columns(&format!(
        r#"{{"subject":["A"],"start":[{NINE}],"end":[{NINE}],"unread":["busy_status","body"]}}"#
    ))
    .unwrap();
    assert_eq!(unread, ["busy_status", "body"]);
}

/// 2026-03-25T09:00:00Z and 09:30 as the script writes them, seconds since 1970.
const NINE: f64 = 1_774_429_200.0;

#[test]
fn reads_the_scripts_lists_as_events() {
    let events = parse_outlook_columns_events(&format!(
        r#"{{"id":[12,13],"subject":["Planning",null],"start":[{NINE},{}],"end":[{},{}],
            "location":["Room 1",null],"organizer":["Alex",null],"is_organizer":[true,false],
            "busy_status":["tentative","out of office"],"body":["Agenda",null]}}"#,
        NINE + 3600.0,
        NINE + 1800.0,
        NINE + 5400.0,
    ))
    .unwrap();

    assert_eq!(events.len(), 2);
    assert_eq!(events[0].id.as_deref(), Some("12"));
    assert_eq!(events[0].subject.as_deref(), Some("Planning"));
    // The spelling event ids have always ended with.
    assert_eq!(events[0].start, "2026-03-25T09:00:00Z");
    assert_eq!(events[0].end, "2026-03-25T09:30:00Z");
    assert_eq!(events[0].location.as_deref(), Some("Room 1"));
    assert_eq!(events[0].organizer.as_deref(), Some("Alex"));
    assert_eq!(events[0].is_organizer, Some(true));
    assert_eq!(events[0].busy_status, Some(BusyStatus::Tentative));
    assert_eq!(events[0].body.as_deref(), Some("Agenda"));
    assert_eq!(events[1].subject, None);
    assert_eq!(events[1].busy_status, Some(BusyStatus::OutOfOffice));
    assert_eq!(events[1].body, None);
}

#[test]
fn lists_outlook_would_not_give_are_left_out() {
    let events = parse_outlook_columns_events(&format!(
        r#"{{"subject":["Planning"],"start":[{NINE}],"end":[{}]}}"#,
        NINE + 1800.0
    ))
    .unwrap();

    let [event] = events.as_slice() else {
        panic!("one event, got {events:?}");
    };
    assert_eq!(
        (&event.id, &event.organizer, &event.busy_status, &event.body),
        (&None, &None, &None, &None)
    );
}

#[test]
fn an_empty_calendar_has_no_events_and_broken_answers_fail() {
    assert!(
        parse_outlook_columns_events(r#"{"subject":[],"start":[],"end":[]}"#)
            .unwrap()
            .is_empty()
    );
    assert!(parse_outlook_columns_events(r#"{"subject":["broken""#).is_err());
    // Lists that do not line up would pair one event's subject with another's time.
    assert!(
        parse_outlook_columns_events(&format!(
            r#"{{"subject":["A","B"],"start":[{NINE}],"end":[{NINE}]}}"#
        ))
        .is_err()
    );
}

#[test]
fn reads_outlook_for_macs_busy_names_and_weighs_them_as_on_windows() {
    for (name, status) in [
        ("free", BusyStatus::Free),
        ("tentative", BusyStatus::Tentative),
        ("busy", BusyStatus::Busy),
        ("out of office", BusyStatus::OutOfOffice),
        ("working elsewhere", BusyStatus::WorkingElsewhere),
    ] {
        assert_eq!(busy_status_from_mac(name), Some(status), "{name}");
    }
    assert_eq!(busy_status_from_mac("«constant ****fbst»"), None);

    let events = parse_outlook_columns_events(&format!(
        r#"{{"subject":["Lunch"],"start":[{NINE}],"end":[{}],"busy_status":["free"],
            "body":["Join: https://teams.microsoft.com/meet/123456789?p=abc"]}}"#,
        NINE + 1800.0
    ))
    .unwrap();
    let normalized = normalize_outlook_events(events, &options("2026-03-25", "2026-03-25"));
    assert_eq!(
        normalized[0].meeting.meeting_weight,
        MeetingWeight::of(Some(BusyStatus::Free))
    );
    assert_eq!(
        normalized[0].meeting.join_url.as_deref(),
        Some("https://teams.microsoft.com/meet/123456789?p=abc")
    );
}

/// Without ids the lists line up by start and subject only: two events that share both could
/// trade their other properties, so those are left out for them.
#[test]
fn events_that_cannot_be_told_apart_without_ids_lose_what_could_be_swapped() {
    let (events, _) = parse_outlook_columns(&format!(
        r#"{{"subject":["Sync","Sync","Other"],"start":[{NINE},{NINE},{NINE}],"end":[{e},{e},{e}],
            "location":["Room A","Room B","Room C"],"organizer":["Ann","Bob","Cat"],
            "is_organizer":[true,false,true],"busy_status":["busy","free","busy"],
            "body":["A's notes","B's notes","C's notes"]}}"#,
        e = NINE + 1800.0
    ))
    .unwrap();

    for event in &events[..2] {
        assert_eq!(
            (
                &event.location,
                &event.organizer,
                &event.is_organizer,
                &event.busy_status,
                &event.body
            ),
            (&None, &None, &None, &None, &None)
        );
    }
    assert_eq!(events[2].location.as_deref(), Some("Room C"));
    assert_eq!(events[2].body.as_deref(), Some("C's notes"));
    assert!(
        crate::diagnostics::entries()
            .iter()
            .any(|log| { log.scope == "outlook" && log.message.contains("2 events") })
    );

    // With ids, the same rows keep everything.
    let (events, _) = parse_outlook_columns(&format!(
        r#"{{"id":[1,2],"subject":["Sync","Sync"],"start":[{NINE},{NINE}],"end":[{NINE},{NINE}],
            "location":["Room A","Room B"]}}"#
    ))
    .unwrap();
    assert_eq!(events[1].location.as_deref(), Some("Room B"));
}

/// An id that is missing or repeated in the read tells its event apart from no other: such
/// events fall back to start and subject.
#[test]
fn missing_or_repeated_ids_count_as_no_id() {
    for ids in ["[null,null,27]", "[13,13,27]"] {
        let (events, _) = parse_outlook_columns(&format!(
            r#"{{"id":{ids},"subject":["Sync","Sync","Other"],"start":[{NINE},{NINE},{NINE}],
                "end":[{NINE},{NINE},{NINE}],"location":["Room A","Room B","Room C"],
                "body":["A's notes","B's notes","C's notes"]}}"#
        ))
        .unwrap();
        for event in &events[..2] {
            assert_eq!((&event.location, &event.body), (&None, &None), "{ids}");
        }
        assert_eq!(events[2].location.as_deref(), Some("Room C"), "{ids}");
    }

    let (events, _) = parse_outlook_columns(&format!(
        r#"{{"id":[13,14],"subject":["Sync","Sync"],"start":[{NINE},{NINE}],"end":[{NINE},{NINE}],
            "location":["Room A","Room B"]}}"#
    ))
    .unwrap();
    assert_eq!(
        events[1].location.as_deref(),
        Some("Room B"),
        "distinct ids keep everything"
    );
}

/// B73: a `MeetingStatus` Outlook would not give is no proof the user organized the event.
#[test]
fn a_meeting_status_that_could_not_be_read_leaves_the_organizer_unknown() {
    assert_eq!(is_organizer_from_com(None), None);
    assert_eq!(is_organizer_from_com(Some(0)), Some(true));
    assert_eq!(is_organizer_from_com(Some(1)), Some(true));
    // Received (3) and cancelled received (7) meetings are someone else's.
    assert_eq!(is_organizer_from_com(Some(3)), Some(false));
    assert_eq!(is_organizer_from_com(Some(7)), Some(false));
}

/// B73: an unknown role stays unknown in the entry and on the wire, never "attendee".
#[test]
fn an_unknown_organizer_is_left_out_of_the_entry() {
    let unknown = RawOutlookEvent {
        id: Some("meeting".to_string()),
        subject: Some("Planning".to_string()),
        start: "2026-03-25T09:00:00Z".to_string(),
        end: "2026-03-25T09:30:00Z".to_string(),
        location: None,
        organizer: Some("Alex".to_string()),
        is_organizer: None,
        is_default_calendar: Some(true),
        busy_status: None,
        body: None,
    };

    let entries = normalize_outlook_events(vec![unknown], &options("2026-03-25", "2026-03-25"));

    assert_eq!(entries[0].is_organizer, None);
    let wire = serde_json::to_value(stamped(&entries[0])).unwrap();
    assert!(wire.get("isOrganizer").is_none(), "{wire}");
}

/// Entries are untagged: with `isOrganizer` optional, only the source tells an Outlook entry
/// from a Calendar one with the same fields.
#[test]
fn an_outlook_entry_without_a_role_and_a_calendar_entry_read_as_their_own_kinds() {
    use crate::model::ActivityEntry;

    let entry = |source: &str| {
        serde_json::from_value::<ActivityEntry>(serde_json::json!({
            "source": source,
            "connection": "default",
            "id": "planning",
            "timestamp": "2026-03-25T09:00:00Z",
            "title": "Planning",
            "durationMinutes": 30,
            "meetingWeight": "busy",
        }))
        .unwrap()
    };

    assert!(matches!(entry("outlook"), ActivityEntry::Outlook(_)));
    assert!(matches!(entry("calendar"), ActivityEntry::Calendar(_)));
}

/// `entry` as the runtime sends it, stamped with its connection.
fn stamped(entry: &crate::model::OutlookEntry) -> crate::model::OutlookEntry {
    let mut entry = entry.clone();
    entry.connection.stamp(&"outlook".into());
    entry
}
