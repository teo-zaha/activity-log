use chrono::{TimeZone, Utc};

use crate::model::SystemEntryType;

use super::{WINDOWS_SYSTEM_EVENTS, macos::parse_last_output, map_windows_events};

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
fn parses_macos_boot_and_shutdown_from_last() {
    let entries = parse_last_output(
        "reboot time                                Thu Mar 26 2026 06:31\nshutdown time                              Wed Mar 25 2026 23:24\n".to_string(),
        Utc.with_ymd_and_hms(2026, 3, 25, 0, 0, 0).unwrap(),
        Utc.with_ymd_and_hms(2026, 3, 26, 23, 59, 59).unwrap(),
    )
    .unwrap();

    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].entry_type, SystemEntryType::Boot);
    assert_eq!(entries[1].entry_type, SystemEntryType::Shutdown);
}
