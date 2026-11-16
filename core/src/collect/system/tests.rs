use chrono::{Local, NaiveDate, TimeZone, Utc};

use crate::model::SystemEntryType;

use super::{
    WINDOWS_SYSTEM_EVENTS,
    macos::parse_last_output,
    map_windows_events,
    windows::{build_event_query, parse_event_xml},
};

#[test]
fn windows_mapping_excludes_sleep_and_wake() {
    let mapped = map_windows_events(
        &[
            crate::model::system::raw_system_event::RawSystemEvent {
                id: 6005,
                ts: "2026-03-25T08:00:00Z".to_string(),
            },
            crate::model::system::raw_system_event::RawSystemEvent {
                id: 42,
                ts: "2026-03-25T08:05:00Z".to_string(),
            },
            crate::model::system::raw_system_event::RawSystemEvent {
                id: 4800,
                ts: "2026-03-25T08:06:00Z".to_string(),
            },
        ],
        WINDOWS_SYSTEM_EVENTS,
    );

    assert_eq!(mapped.len(), 3);
    assert_eq!(mapped[0].entry_type, SystemEntryType::Boot);
    assert_eq!(mapped[1].label, "Event 42");
    assert_eq!(mapped[2].label, "Event 4800");
}

#[test]
fn builds_windows_event_query_with_ids_and_utc_bounds() {
    let query = build_event_query(
        &[6005, 6006, 6008],
        Utc.with_ymd_and_hms(2026, 3, 25, 8, 0, 0).unwrap(),
        Utc.with_ymd_and_hms(2026, 3, 25, 18, 0, 0).unwrap(),
    );

    assert_eq!(
        query,
        "*[System[(EventID=6005 or EventID=6006 or EventID=6008) and TimeCreated[@SystemTime >= '2026-03-25T08:00:00Z' and @SystemTime < '2026-03-25T18:00:00Z']]]"
    );
}

#[test]
fn builds_windows_event_query_with_single_id() {
    let query = build_event_query(
        &[6005],
        Utc.with_ymd_and_hms(2026, 3, 25, 0, 0, 0).unwrap(),
        Utc.with_ymd_and_hms(2026, 3, 25, 23, 59, 59).unwrap(),
    );

    assert_eq!(
        query,
        "*[System[(EventID=6005) and TimeCreated[@SystemTime >= '2026-03-25T00:00:00Z' and @SystemTime < '2026-03-25T23:59:59Z']]]"
    );
}

#[test]
fn builds_windows_event_query_with_empty_ids() {
    let query = build_event_query(
        &[],
        Utc.with_ymd_and_hms(2026, 3, 25, 8, 0, 0).unwrap(),
        Utc.with_ymd_and_hms(2026, 3, 25, 18, 0, 0).unwrap(),
    );

    assert_eq!(
        query,
        "*[System[() and TimeCreated[@SystemTime >= '2026-03-25T08:00:00Z' and @SystemTime < '2026-03-25T18:00:00Z']]]"
    );
}

#[test]
fn parses_windows_event_xml_with_single_or_double_quoted_timestamps() {
    let single = parse_event_xml(
        r#"<Event><System><EventID Qualifiers="">6005</EventID><TimeCreated SystemTime='2026-03-25T08:00:00.0000000Z'/></System></Event>"#,
    )
    .unwrap()
    .unwrap();
    let double = parse_event_xml(
        r#"<Event><System><EventID>6006</EventID><TimeCreated SystemTime="2026-03-25T18:00:00.0000000Z"/></System></Event>"#,
    )
    .unwrap()
    .unwrap();

    assert_eq!(single.id, 6005);
    assert_eq!(single.ts, "2026-03-25T08:00:00+00:00");
    assert_eq!(double.id, 6006);
    assert_eq!(double.ts, "2026-03-25T18:00:00+00:00");
}

#[test]
fn ignores_windows_event_xml_without_required_fields() {
    let event =
        parse_event_xml(r#"<Event><System><Provider Name="EventLog"/></System></Event>"#).unwrap();

    assert!(event.is_none());
}

#[test]
fn parses_namespaced_windows_event_xml() {
    let event = parse_event_xml(
        r#"<Event xmlns="http://schemas.microsoft.com/win/2004/08/events/event"><System><Provider Name="EventLog"/><EventID Qualifiers="32768">6005</EventID><TimeCreated SystemTime="2026-03-25T08:00:00.1234567Z"/></System></Event>"#,
    )
    .unwrap()
    .unwrap();

    assert_eq!(event.id, 6005);
    assert_eq!(event.ts, "2026-03-25T08:00:00.123456700+00:00");
}

#[test]
fn ignores_windows_event_xml_missing_timestamp() {
    let event =
        parse_event_xml(r#"<Event><System><EventID>6005</EventID></System></Event>"#).unwrap();

    assert!(event.is_none());
}

#[test]
fn errors_on_windows_event_xml_with_malformed_timestamp() {
    let result = parse_event_xml(
        r#"<Event><System><EventID>6005</EventID><TimeCreated SystemTime="not-a-timestamp"/></System></Event>"#,
    );

    assert!(result.is_err());
}

#[test]
fn errors_on_windows_event_xml_with_non_numeric_id() {
    let result = parse_event_xml(
        r#"<Event><System><EventID>boot</EventID><TimeCreated SystemTime="2026-03-25T08:00:00.0000000Z"/></System></Event>"#,
    );

    assert!(result.is_err());
}

/// `last` prints local wall-clock times; convert the same way the parser does.
fn local_rfc3339(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> String {
    let naive = NaiveDate::from_ymd_opt(year, month, day)
        .unwrap()
        .and_hms_opt(hour, minute, 0)
        .unwrap();
    Local
        .from_local_datetime(&naive)
        .single()
        .unwrap()
        .with_timezone(&Utc)
        .to_rfc3339()
}

#[test]
fn parses_macos_last_output() {
    let entries = parse_last_output(
        "reboot time                                Fri May 15 2026 14:23\nshutdown time                              Wed May 13 2026 17:23\nwtmp begins Mon May 11 22:14:48 CEST 2026\n".to_string(),
        Utc.with_ymd_and_hms(2026, 3, 25, 0, 0, 0).unwrap(),
        Utc.with_ymd_and_hms(2026, 5, 20, 23, 59, 59).unwrap(),
    )
    .unwrap();

    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].entry_type, SystemEntryType::Boot);
    assert_eq!(entries[0].timestamp, local_rfc3339(2026, 5, 15, 14, 23));
    assert_eq!(entries[1].entry_type, SystemEntryType::Shutdown);
    assert_eq!(entries[1].timestamp, local_rfc3339(2026, 5, 13, 17, 23));
}

#[test]
fn macos_last_output_stops_at_the_first_entry_before_the_range() {
    // The trailing line is malformed; it must never be parsed because reading stops earlier.
    let entries = parse_last_output(
        "reboot time                                Fri May 15 2026 14:23\nshutdown time                              Wed Mar 11 2026 17:23\nreboot time                                not a date\n".to_string(),
        Utc.with_ymd_and_hms(2026, 3, 25, 0, 0, 0).unwrap(),
        Utc.with_ymd_and_hms(2026, 5, 20, 23, 59, 59).unwrap(),
    )
    .unwrap();

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].entry_type, SystemEntryType::Boot);
}
