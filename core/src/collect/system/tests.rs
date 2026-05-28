use chrono::{TimeZone, Utc};

use crate::model::SystemEntryType;

use super::{
    WINDOWS_SYSTEM_EVENTS, macos::parse_last_output, map_windows_events, parse_raw_system_events,
};

#[test]
fn parses_windows_event_json_payloads() {
    let events = parse_raw_system_events(
        r#"[{"id":6005,"ts":"2026-03-25T08:00:00Z"},{"id":6006,"ts":"2026-03-25T18:00:00Z"}]"#,
    )
    .unwrap();

    assert_eq!(events.len(), 2);
    assert_eq!(events[0].id, 6005);
    assert_eq!(events[1].ts, "2026-03-25T18:00:00Z");
}

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
fn parses_macos_last_output() {
    let entries = parse_last_output(
        "reboot time                                Fri May 15 2026 14:23\nshutdown time                              Wed May 13 2026 17:23\nwtmp begins Mon May 11 22:14:48 CEST 2026\n".to_string(),
        Utc.with_ymd_and_hms(2026, 3, 25, 0, 0, 0).unwrap(),
        Utc.with_ymd_and_hms(2026, 5, 20, 23, 59, 59).unwrap(),
    )
    .unwrap();

    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].entry_type, SystemEntryType::Boot);
    assert_eq!(
        entries[0].timestamp,
        Utc.with_ymd_and_hms(2026, 5, 15, 12, 23, 0)
            .unwrap()
            .to_rfc3339()
    );
    assert_eq!(entries[1].entry_type, SystemEntryType::Shutdown);
    assert_eq!(
        entries[1].timestamp,
        Utc.with_ymd_and_hms(2026, 5, 13, 15, 23, 0)
            .unwrap()
            .to_rfc3339()
    );
}
