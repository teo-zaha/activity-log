use chrono::{Local, NaiveDate, TimeZone, Utc};

use crate::model::SystemEntryType;

use super::{
    WINDOWS_SYSTEM_EVENTS,
    macos::{LastLine, LastReader, parse_last_output, parse_power_log},
    map_windows_events,
    windows::{build_event_query, parse_event_xml},
};

/// B72: the dates of `last` are parsed as English, so it must print them so.
#[cfg(target_os = "macos")]
#[test]
fn last_and_pmset_print_in_the_c_locale() {
    use super::macos::{last_command, power_log_command};
    for command in [last_command(), power_log_command()] {
        let locale = command
            .as_std()
            .get_envs()
            .find(|(name, _)| *name == "LC_ALL")
            .and_then(|(_, value)| value);
        assert_eq!(locale, Some(std::ffi::OsStr::new("C")), "{command:?}");
    }
}

/// S27: an event id without a mapping (sleep and wake are not asked for) is left out, never
/// shown as a boot.
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

    assert_eq!(mapped.len(), 1);
    assert_eq!(mapped[0].entry_type, SystemEntryType::Boot);
    assert_eq!(mapped[0].label, "System started");
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
        Local,
    );

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
        Local,
    );

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].entry_type, SystemEntryType::Boot);
}

/// A synthetic `pmset -g log`: the type column padded to 20 characters and a tab, as macOS
/// writes it.
fn power_line(time: &str, kind: &str, details: &str) -> String {
    format!("{time} {kind:<20}\t{details}")
}

fn power_log() -> String {
    [
        "Time stamp                Domain              Message".to_string(),
        power_line("2026-11-09 08:00:00 +0100", "Sleep", "Entering Sleep state due to 'Idle Sleep':TCPKeepAlive=active Using AC (Charge:80%) 2 secs"),
        power_line("2026-11-10 07:59:00 +0100", "Sleep", "Entering Sleep state due to 'Idle Sleep':TCPKeepAlive=active Using Batt (Charge:79%) 2 secs"),
        power_line("2026-11-10 09:00:00 +0100", "Sleep", "Entering Sleep state due to 'Clamshell Sleep':TCPKeepAlive=active Using Batt (Charge:78%) 3 secs"),
        power_line("2026-11-10 09:05:00 +0100", "Wake Requests", "[*process=dasd request=SleepService deltaSecs=600 wakeAt=2026-11-10 09:15:00]"),
        power_line("2026-11-10 09:15:00 +0100", "DarkWake", "DarkWake from Deep Idle [CDN] : due to RTC/Maintenance Using AC (Charge:78%) 10 secs"),
        power_line("2026-11-10 09:15:10 +0100", "Sleep", "Entering Sleep state due to 'Maintenance Sleep':TCPKeepAlive=active Using AC (Charge:78%) 4 secs"),
        power_line("2026-11-10 09:30:00 +0100", "Sleep", "Entering Sleep state due to 'Sleep Service Back to Sleep':TCPKeepAlive=active Using AC (Charge:78%) 1 secs"),
        power_line("2026-11-10 09:40:00 +0100", "Sleep", "Entering DarkWake state due to 'Notification Wake Back to Sleep':TCPKeepAlive=active Using AC (Charge:78%) 1 secs"),
        power_line("2026-11-10 10:00:00 +0100", "Wake", "Wake from Deep Idle [CDNVA] : due to smc.sysState.Wake(0x1) lid SMC.OutboxNotEmpty Using AC (Charge:78%)"),
        power_line("2026-11-10 10:00:01 +0100", "WakeDetails", "DriverReason:smc.sysState.Wake - DriverDetails: lid"),
        power_line("2026-11-10 10:00:02 +0100", "WakeTime", "WakeTime: 1.2 sec"),
        power_line("2026-11-10 12:00:00 -0500", "Wake", "DarkWake to FullWake from Deep Idle [CDNVA] : due to UserActivity Assertion Using BATT (Charge:70%)"),
        power_line("2026-11-10 18:30:00 +0100", "Wake", "DarkWake to FullWake from Deep Idle [CDNVA] : due to HID Activity Using AC (Charge:70%)"),
        power_line("2026-11-10 19:00:00 +0100", "Wake", "Wake from Deep Idle [CDNVA] : due to smc.sysState.Wake(0x1) pwrbtn SMC.OutboxNotEmpty Using AC"),
        power_line("2026-11-10 20:00:00 +0100", "Wake", "DarkWake to FullWake from Deep Idle [CDNVAP] : due to Notification Using AC (Charge:70%) 3 secs"),
        power_line("2026-11-10 21:00:00 +0100", "Wake", "Wake from Deep Idle [CDNVA] : due to smc.sysState.Wake(0x1) wifibt SMC.OutboxNotEmpty Using AC"),
        power_line("2026-11-10 22:00:00 +0100", "Sleep", "Entering Sleep state due to 'Software Sleep pid=1':TCPKeepAlive=active Using AC"),
        power_line("2026-11-10 22:30:00 +0100", "Sleep", "Entering Sleep state due to 'Thermal Emergency Sleep':TCPKeepAlive=active"),
        "  Total Sleep/Wakes since boot at 2026-11-01 08:00:00 +0100 :12".to_string(),
        power_line("2026-11-11 00:00:00 +0100", "Sleep", "Entering Sleep state due to 'Idle Sleep':TCPKeepAlive=active Using AC (Charge:70%) 2 secs"),
        power_line("2026-11-11 01:00:00 +0100", "Wake", "Wake from Deep Idle [CDNVA] : due to smc.sysState.Wake(0x1) lid"),
    ]
    .join("\n")
}

/// 10 November in UTC+1.
fn tenth_of_november() -> (chrono::DateTime<Utc>, chrono::DateTime<Utc>) {
    (
        Utc.with_ymd_and_hms(2026, 11, 9, 23, 0, 0).unwrap(),
        Utc.with_ymd_and_hms(2026, 11, 10, 23, 0, 0).unwrap(),
    )
}

#[test]
fn reads_user_sleeps_and_full_wakes_from_the_power_log() {
    let (start, end) = tenth_of_november();
    let entries = parse_power_log(&power_log(), start, end);

    let read = entries
        .iter()
        .map(|entry| {
            (
                entry.entry_type.clone(),
                entry.timestamp.as_str(),
                entry.label.as_str(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        read,
        [
            (
                SystemEntryType::Sleep,
                "2026-11-10T06:59:00+00:00",
                "Sleep (idle)"
            ),
            (
                SystemEntryType::Sleep,
                "2026-11-10T08:00:00+00:00",
                "Sleep (lid closed)"
            ),
            (
                SystemEntryType::Wake,
                "2026-11-10T09:00:00+00:00",
                "Wake (lid opened)"
            ),
            // Its own offset: noon at UTC-5 is 17:00 UTC.
            (
                SystemEntryType::Wake,
                "2026-11-10T17:00:00+00:00",
                "Wake (user activity)"
            ),
            (
                SystemEntryType::Wake,
                "2026-11-10T17:30:00+00:00",
                "Wake (keyboard or mouse)"
            ),
            (
                SystemEntryType::Wake,
                "2026-11-10T18:00:00+00:00",
                "Wake (power button)"
            ),
            (
                SystemEntryType::Wake,
                "2026-11-10T19:00:00+00:00",
                "Wake (notification)"
            ),
            (SystemEntryType::Wake, "2026-11-10T20:00:00+00:00", "Wake"),
            (
                SystemEntryType::Sleep,
                "2026-11-10T21:00:00+00:00",
                "Sleep (requested)"
            ),
            // A reason without a label of its own reads as itself.
            (
                SystemEntryType::Sleep,
                "2026-11-10T21:30:00+00:00",
                "Sleep (thermal emergency)"
            ),
        ]
    );
    assert_eq!(entries[1].id, "sleep/2026-11-10T08:00:00+00:00");
}

#[test]
fn leaves_out_maintenance_sleeps_dark_wakes_and_wake_bookkeeping() {
    let (start, end) = tenth_of_november();
    let labels = parse_power_log(&power_log(), start, end)
        .into_iter()
        .map(|entry| entry.label)
        .collect::<Vec<_>>();

    assert!(!labels.iter().any(|label| label.contains("maintenance")));
    assert!(!labels.iter().any(|label| label.contains("back to")));
    // Nothing from 09:05 to 09:40 (wake requests, the dark wake, maintenance and back-to-sleep).
    let quiet_hour = parse_power_log(
        &power_log(),
        Utc.with_ymd_and_hms(2026, 11, 10, 8, 1, 0).unwrap(),
        Utc.with_ymd_and_hms(2026, 11, 10, 8, 59, 0).unwrap(),
    );
    assert!(quiet_hour.is_empty(), "{quiet_hour:?}");
    // WakeDetails and WakeTime right after the wake add nothing.
    let after_wake = parse_power_log(
        &power_log(),
        Utc.with_ymd_and_hms(2026, 11, 10, 9, 0, 0).unwrap(),
        Utc.with_ymd_and_hms(2026, 11, 10, 9, 1, 0).unwrap(),
    );
    assert_eq!(after_wake.len(), 1);
}

#[test]
fn stops_at_the_first_sleep_or_wake_after_the_range() {
    use super::macos::{PowerLine, classify_power_line};

    let (start, end) = tenth_of_november();
    let line = power_line(
        "2026-11-11 00:00:00 +0100",
        "Sleep",
        "Entering Sleep state due to 'Idle Sleep':TCPKeepAlive=active",
    );
    assert!(matches!(
        classify_power_line(&line, start, end),
        PowerLine::AfterRange
    ));
    let before = power_line(
        "2026-11-09 08:00:00 +0100",
        "Wake",
        "Wake from Deep Idle : due to lid",
    );
    assert!(matches!(
        classify_power_line(&before, start, end),
        PowerLine::Skip
    ));
    for junk in [
        "",
        "Time stamp                Domain",
        "2026-11-10 not a time Sleep \tEntering Sleep state",
    ] {
        assert!(
            matches!(classify_power_line(junk, start, end), PowerLine::Skip),
            "{junk}"
        );
    }
}

/// In Europe/Berlin 2026-10-25 02:30 happens twice and 2026-03-29 02:30 never.
#[test]
fn macos_last_keeps_an_ambiguous_hour_and_skips_unreadable_lines() {
    let berlin = chrono_tz::Europe::Berlin;
    let entries = parse_last_output(
        [
            "reboot time                                Mon Oct 26 2026 08:00",
            "shutdown time                              Sun Oct 25 2026 02:30",
            "reboot time                                Sun Oct 25 2026 01:15",
            "shutdown time                              Sun Mar 29 2026 02:30",
            "reboot time                                not a date",
            "shutdown time                              Sat Oct 24 2026 18:00",
        ]
        .join("\n"),
        Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap(),
        Utc.with_ymd_and_hms(2026, 11, 1, 0, 0, 0).unwrap(),
        berlin,
    );

    let timestamps = entries
        .iter()
        .map(|entry| entry.timestamp.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        timestamps,
        [
            "2026-10-26T07:00:00+00:00",
            // The later of the two 02:30s: nothing newer rules it out.
            "2026-10-25T01:30:00+00:00",
            "2026-10-24T23:15:00+00:00",
            "2026-10-24T16:00:00+00:00",
        ]
    );
    // The collector names what it skipped in the Logs pane.
    let skipped = LastReader::new(
        Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap(),
        Utc.with_ymd_and_hms(2026, 11, 1, 0, 0, 0).unwrap(),
        berlin,
    )
    .line("shutdown time                              Sun Mar 29 2026 02:30");
    assert!(matches!(skipped, LastLine::Unreadable(time) if time == "Sun Mar 29 2026 02:30"));
}

/// Newest first, 02:05 in the second 02 o'clock hour, then 02:50 in the first: the order tells
/// the two apart, and neither ends the scan early.
#[test]
fn macos_last_reads_a_repeated_hour_by_the_order_of_the_records() {
    let berlin = chrono_tz::Europe::Berlin;
    let entries = parse_last_output(
        [
            "reboot time                                Sun Oct 25 2026 02:05",
            "shutdown time                              Sun Oct 25 2026 02:50",
        ]
        .join("\n"),
        Utc.with_ymd_and_hms(2026, 10, 25, 0, 30, 0).unwrap(),
        Utc.with_ymd_and_hms(2026, 10, 26, 0, 0, 0).unwrap(),
        berlin,
    );

    let timestamps = entries
        .iter()
        .map(|entry| entry.timestamp.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        timestamps,
        ["2026-10-25T01:05:00+00:00", "2026-10-25T00:50:00+00:00"]
    );
}

/// A lone record in the repeated hour: the reading inside the range is taken; when both are
/// inside and nothing decides, the later one is, and the reader says so.
#[test]
fn macos_last_takes_the_reading_in_the_range_and_reports_one_it_cannot_decide() {
    let berlin = chrono_tz::Europe::Berlin;
    let line = "shutdown time                              Sun Oct 25 2026 02:50";

    // Only the first 02:50 (00:50Z) is in the range.
    let mut first_fold = LastReader::new(
        Utc.with_ymd_and_hms(2026, 10, 25, 0, 30, 0).unwrap(),
        Utc.with_ymd_and_hms(2026, 10, 25, 1, 0, 0).unwrap(),
        berlin,
    );
    let LastLine::Entry(entry) = first_fold.line(line) else {
        panic!("the record is in the range");
    };
    assert_eq!(entry.timestamp, "2026-10-25T00:50:00+00:00");
    assert!(first_fold.undecided().is_empty());

    let mut whole_day = LastReader::new(
        Utc.with_ymd_and_hms(2026, 10, 24, 22, 0, 0).unwrap(),
        Utc.with_ymd_and_hms(2026, 10, 25, 23, 0, 0).unwrap(),
        berlin,
    );
    let LastLine::Entry(entry) = whole_day.line(line) else {
        panic!("the record is in the range");
    };
    assert_eq!(entry.timestamp, "2026-10-25T01:50:00+00:00");
    assert_eq!(whole_day.undecided(), ["Sun Oct 25 2026 02:50"]);
}
