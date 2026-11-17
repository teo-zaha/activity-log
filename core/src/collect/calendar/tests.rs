use std::str::FromStr;

use calcard::common::timezone::Tz;
use chrono::{TimeZone, Utc};

use super::{FeedLocation, feed_location, parse_events, read_feed};
use crate::model::CollectOptions;

/// 2–8 March 2026, UTC.
fn week() -> CollectOptions {
    CollectOptions {
        start: Utc.with_ymd_and_hms(2026, 3, 2, 0, 0, 0).unwrap(),
        end: Utc.with_ymd_and_hms(2026, 3, 9, 0, 0, 0).unwrap(),
    }
}

/// The shape Outlook publishes: Windows zone names with their own VTIMEZONE.
const OUTLOOK_FEED: &str = "BEGIN:VCALENDAR\r
PRODID:Microsoft Exchange Server 2010\r
VERSION:2.0\r
BEGIN:VTIMEZONE\r
TZID:W. Europe Standard Time\r
BEGIN:STANDARD\r
DTSTART:16010101T030000\r
TZOFFSETFROM:+0200\r
TZOFFSETTO:+0100\r
RRULE:FREQ=YEARLY;INTERVAL=1;BYDAY=-1SU;BYMONTH=10\r
END:STANDARD\r
BEGIN:DAYLIGHT\r
DTSTART:16010101T020000\r
TZOFFSETFROM:+0100\r
TZOFFSETTO:+0200\r
RRULE:FREQ=YEARLY;INTERVAL=1;BYDAY=-1SU;BYMONTH=3\r
END:DAYLIGHT\r
END:VTIMEZONE\r
BEGIN:VEVENT\r
UID:standup\r
SUMMARY:Standup\r
ORGANIZER;CN=Team Lead:mailto:lead@example.com\r
LOCATION:Room 1\r
RRULE:FREQ=WEEKLY;BYDAY=MO,WE,FR\r
EXDATE;TZID=W. Europe Standard Time:20260304T093000\r
DTSTART;TZID=W. Europe Standard Time:20250106T093000\r
DTEND;TZID=W. Europe Standard Time:20250106T094500\r
END:VEVENT\r
BEGIN:VEVENT\r
UID:standup\r
RECURRENCE-ID;TZID=W. Europe Standard Time:20260306T093000\r
SUMMARY:Standup (moved)\r
DTSTART;TZID=W. Europe Standard Time:20260306T110000\r
DTEND;TZID=W. Europe Standard Time:20260306T111500\r
END:VEVENT\r
BEGIN:VEVENT\r
UID:review\r
SUMMARY:Review\r
ORGANIZER:mailto:me@example.com\r
DTSTART:20260303T140000Z\r
DTEND:20260303T150000Z\r
END:VEVENT\r
BEGIN:VEVENT\r
UID:holiday\r
SUMMARY:Holiday\r
DTSTART;VALUE=DATE:20260305\r
DTEND;VALUE=DATE:20260306\r
END:VEVENT\r
BEGIN:VEVENT\r
UID:cancelled\r
SUMMARY:Cancelled sync\r
STATUS:CANCELLED\r
DTSTART:20260303T080000Z\r
DTEND:20260303T083000Z\r
END:VEVENT\r
BEGIN:VEVENT\r
UID:old\r
SUMMARY:Last year\r
DTSTART:20250303T080000Z\r
DTEND:20250303T083000Z\r
END:VEVENT\r
END:VCALENDAR\r
";

#[test]
fn expands_an_outlook_feed_into_occurrences_in_the_range() {
    let entries = parse_events(OUTLOOK_FEED, &week(), Tz::UTC).unwrap();

    let summary = entries
        .iter()
        .map(|entry| {
            (
                entry.timestamp.as_str(),
                entry.title.as_str(),
                entry.duration_minutes,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        summary,
        [
            // Monday 09:30 Berlin (CET, UTC+1).
            ("2026-03-02T08:30:00+00:00", "Standup", 15),
            ("2026-03-03T14:00:00+00:00", "Review", 60),
            // Wednesday is an EXDATE; Friday was moved to 11:00.
            ("2026-03-06T10:00:00+00:00", "Standup (moved)", 15),
        ]
    );
    assert_eq!(entries[0].organizer.as_deref(), Some("Team Lead"));
    assert_eq!(entries[0].location.as_deref(), Some("Room 1"));
    assert_eq!(entries[1].organizer.as_deref(), Some("me@example.com"));
    assert!(entries.iter().all(|entry| entry.source == "calendar"));
}

#[test]
fn an_endless_daily_series_stops_at_the_range() {
    let feed = "BEGIN:VCALENDAR\r
VERSION:2.0\r
BEGIN:VEVENT\r
UID:daily\r
SUMMARY:Daily\r
RRULE:FREQ=DAILY\r
DTSTART:20100101T070000Z\r
DTEND:20100101T071500Z\r
END:VEVENT\r
END:VCALENDAR\r
";

    let entries = parse_events(feed, &week(), Tz::UTC).unwrap();

    assert_eq!(entries.len(), 7);
    assert_eq!(entries[0].timestamp, "2026-03-02T07:00:00+00:00");
    assert_eq!(entries[6].timestamp, "2026-03-08T07:00:00+00:00");
}

#[test]
fn reads_times_without_a_zone_in_an_explicit_zone() {
    let feed = "BEGIN:VCALENDAR\r
VERSION:2.0\r
BEGIN:VEVENT\r
UID:floating\r
SUMMARY:Lunch\r
DTSTART:20260304T120000\r
DTEND:20260304T130000\r
END:VEVENT\r
END:VCALENDAR\r
";

    let entries = parse_events(feed, &week(), Tz::from_str("Europe/Berlin").unwrap()).unwrap();

    assert_eq!(entries[0].timestamp, "2026-03-04T11:00:00+00:00");
}

#[test]
fn rejects_text_that_is_not_a_calendar() {
    assert!(parse_events("<html>Sign in</html>", &week(), Tz::UTC).is_err());
}

#[test]
fn accepts_https_webcal_and_files_but_not_http() {
    assert_eq!(
        feed_location(" webcal://calendar.example.com/a.ics ").unwrap(),
        FeedLocation::Url("https://calendar.example.com/a.ics".parse().unwrap())
    );
    assert!(matches!(
        feed_location("https://outlook.office365.com/owa/calendar/x/calendar.ics").unwrap(),
        FeedLocation::Url(_)
    ));
    let file = std::env::temp_dir().join("calendar.ics");
    assert_eq!(
        feed_location(&file.display().to_string()).unwrap(),
        FeedLocation::File(file)
    );
    assert!(feed_location("http://calendar.example.com/a.ics").is_err());
    assert!(feed_location("calendar.ics").is_err());
}

#[tokio::test]
async fn errors_never_contain_the_link() {
    // Nothing listens on port 9 (discard), so the request fails fast.
    let err = read_feed("https://127.0.0.1:9/secret-token/calendar.ics")
        .await
        .unwrap_err();

    let message = format!("{err:#}");
    assert!(!message.contains("secret-token"), "{message}");
    assert!(message.contains("127.0.0.1"), "{message}");
}

#[tokio::test]
async fn reads_a_local_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("export.ics");
    std::fs::write(&path, OUTLOOK_FEED).unwrap();

    let text = read_feed(&path.display().to_string()).await.unwrap();

    assert!(text.starts_with("BEGIN:VCALENDAR"));
}

fn feed(events: &str) -> String {
    format!("BEGIN:VCALENDAR\r\nVERSION:2.0\r\n{events}END:VCALENDAR\r\n")
}

#[test]
fn a_moved_override_replaces_the_original_despite_a_different_sequence() {
    let text = feed(
        "BEGIN:VEVENT
UID:series
SEQUENCE:9
SUMMARY:Original
DTSTART:20260302T080000Z
DTEND:20260302T083000Z
RRULE:FREQ=DAILY;COUNT=3
END:VEVENT
BEGIN:VEVENT
UID:series
SEQUENCE:1
RECURRENCE-ID:20260303T080000Z
SUMMARY:Moved
DTSTART:20260305T100000Z
DTEND:20260305T110000Z
END:VEVENT
",
    );
    let entries = parse_events(&text, &week(), Tz::UTC).unwrap();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].timestamp, "2026-03-02T08:00:00+00:00");
    assert_eq!(entries[1].timestamp, "2026-03-04T08:00:00+00:00");
    assert_eq!(entries[2].timestamp, "2026-03-05T10:00:00+00:00");
    assert_eq!(entries[2].title, "Moved");
    assert_eq!(entries[2].duration_minutes, 60);
    let original_window = CollectOptions {
        start: Utc.with_ymd_and_hms(2026, 3, 3, 0, 0, 0).unwrap(),
        end: Utc.with_ymd_and_hms(2026, 3, 4, 0, 0, 0).unwrap(),
    };
    assert!(
        parse_events(&text, &original_window, Tz::UTC)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn overrides_with_the_same_sequence_and_time_stay_with_their_uid() {
    for sequence in ["", "SEQUENCE:7\n"] {
        let mut events = String::new();
        for (uid, hour) in [("alpha", 10), ("beta", 11)] {
            events.push_str(&format!(
                "BEGIN:VEVENT
UID:{uid}
{sequence}SUMMARY:{uid}
DTSTART:20260302T080000Z
DTEND:20260302T083000Z
RRULE:FREQ=DAILY;COUNT=2
END:VEVENT
BEGIN:VEVENT
UID:{uid}
{sequence}RECURRENCE-ID:20260303T080000Z
SUMMARY:{uid} moved
DTSTART:20260303T{hour}0000Z
DTEND:20260303T{hour}3000Z
END:VEVENT
"
            ));
        }
        let entries = parse_events(&feed(&events), &week(), Tz::UTC).unwrap();
        assert_eq!(entries.len(), 4);
        assert_eq!(entries[2].title, "alpha moved");
        assert_eq!(entries[2].timestamp, "2026-03-03T10:00:00+00:00");
        assert_eq!(entries[3].title, "beta moved");
        assert_eq!(entries[3].timestamp, "2026-03-03T11:00:00+00:00");
    }
}

#[test]
fn a_cancelled_override_with_a_different_sequence_suppresses_the_original() {
    let text = feed(
        "BEGIN:VEVENT
UID:series
SEQUENCE:3
DTSTART:20260302T080000Z
DTEND:20260302T083000Z
RRULE:FREQ=DAILY;COUNT=2
END:VEVENT
BEGIN:VEVENT
UID:series
SEQUENCE:4
RECURRENCE-ID:20260303T080000Z
STATUS:CANCELLED
END:VEVENT
",
    );
    let entries = parse_events(&text, &week(), Tz::UTC).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].timestamp, "2026-03-02T08:00:00+00:00");
}

#[test]
fn windows_zone_recurrences_and_overrides_cross_dst_and_include_utc_until() {
    let timezone = OUTLOOK_FEED.split("BEGIN:VEVENT").next().unwrap();
    let text = format!(
        "{timezone}BEGIN:VEVENT
UID:dst-series
SEQUENCE:3
DTSTART;TZID=W. Europe Standard Time:20260323T093000
DTEND;TZID=W. Europe Standard Time:20260323T100000
RRULE:FREQ=WEEKLY;UNTIL=20260406T073000Z;BYDAY=MO
END:VEVENT
BEGIN:VEVENT
UID:dst-series
SEQUENCE:4
RECURRENCE-ID;TZID=W. Europe Standard Time:20260330T093000
DTSTART;TZID=W. Europe Standard Time:20260330T103000
DTEND;TZID=W. Europe Standard Time:20260330T110000
END:VEVENT
END:VCALENDAR
"
    );
    let options = CollectOptions {
        start: Utc.with_ymd_and_hms(2026, 3, 20, 0, 0, 0).unwrap(),
        end: Utc.with_ymd_and_hms(2026, 4, 10, 0, 0, 0).unwrap(),
    };
    let entries = parse_events(&text, &options, Tz::UTC).unwrap();
    assert_eq!(
        entries
            .iter()
            .map(|e| e.timestamp.as_str())
            .collect::<Vec<_>>(),
        [
            "2026-03-23T08:30:00+00:00",
            "2026-03-30T08:30:00+00:00",
            "2026-04-06T07:30:00+00:00",
        ]
    );
    assert!(entries.iter().all(|e| e.duration_minutes == 30));
}

#[test]
fn utc_until_excludes_a_later_occurrence_in_a_western_zone() {
    let text = feed(
        "BEGIN:VEVENT
UID:western
DTSTART;TZID=America/New_York:20260302T090000
DTEND;TZID=America/New_York:20260302T093000
RRULE:FREQ=DAILY;UNTIL=20260303T135959Z
END:VEVENT
",
    );
    let entries = parse_events(&text, &week(), Tz::UTC).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].timestamp, "2026-03-02T14:00:00+00:00");
}

#[test]
fn expansion_errors_are_logged_by_uid_and_do_not_hide_valid_events() {
    let text = feed(
        "BEGIN:VEVENT
UID:calendar-error-missing-start
SUMMARY:Private subject
END:VEVENT
BEGIN:VEVENT
UID:calendar-error-invalid-rule
SUMMARY:Another private subject
DTSTART:20260302T080000Z
RRULE:FREQ=DAILY;UNTIL=20260301T080000Z
END:VEVENT
BEGIN:VEVENT
UID:healthy
DTSTART:20260302T100000Z
DTEND:20260302T103000Z
END:VEVENT
",
    );
    let entries = parse_events(&text, &week(), Tz::UTC).unwrap();
    assert_eq!(entries.len(), 1);
    let logs = crate::diagnostics::entries();
    for uid in [
        "calendar-error-missing-start",
        "calendar-error-invalid-rule",
    ] {
        let log = logs.iter().find(|log| log.message.contains(uid)).unwrap();
        assert_eq!(log.scope, "calendar");
        assert_eq!(log.level, crate::model::LogLevel::Warn);
        assert!(!log.message.contains("subject"));
    }
}

#[tokio::test]
async fn collection_reads_times_without_a_zone_in_the_machine_zone() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("floating.ics");
    std::fs::write(
        &path,
        feed(
            "BEGIN:VEVENT
UID:floating
SUMMARY:Lunch
DTSTART:20260304T120000
DTEND:20260304T130000
END:VEVENT
",
        ),
    )
    .unwrap();

    let entries = super::collect_calendar(path.to_str().unwrap(), &week())
        .await
        .unwrap();

    assert_eq!(entries.len(), 1);
}

#[test]
fn unresolved_zones_and_floating_recurrence_periods_do_not_become_instants() {
    for dates in [
        "DTSTART;TZID=Unknown/Zone:20260302T090000\nDTEND;TZID=Unknown/Zone:20260302T100000\n",
        "DTSTART:20260302T090000Z\nRDATE;VALUE=PERIOD:20260303T090000/20260303T100000\n",
        "DTSTART:20260302T090000Z\nRDATE;VALUE=PERIOD:20260303T090000/PT1H\n",
    ] {
        let text = feed(&format!(
            "BEGIN:VEVENT\nUID:unresolved\n{dates}END:VEVENT\n"
        ));
        let error = parse_events(&text, &week(), Tz::Floating).unwrap_err();
        assert!(error.to_string().contains("floating or unknown time zone"));
    }
}
