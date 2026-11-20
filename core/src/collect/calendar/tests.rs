use std::{collections::BTreeSet, str::FromStr, sync::atomic::AtomicBool};

use calcard::common::timezone::Tz;
use chrono::{TimeZone, Utc};

use super::{FeedLocation, check_feed, feed_location, parse_events, read_feed};
use crate::model::{BusyStatus, CollectOptions, MeetingWeight};

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
                entry.title.as_deref().unwrap(),
                entry.meeting.duration_minutes,
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
    assert_eq!(entries[2].title.as_deref(), Some("Moved"));
    assert_eq!(entries[2].meeting.duration_minutes, 60);
    // Occurrences by their start; the moved one by the occurrence it replaces.
    assert_eq!(
        entries.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
        [
            "series/2026-03-02T08:00:00+00:00",
            "series/2026-03-04T08:00:00+00:00",
            "series/2026-03-03T08:00:00+00:00",
        ]
    );
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
        assert_eq!(entries[2].title.as_deref(), Some("alpha moved"));
        assert_eq!(entries[2].timestamp, "2026-03-03T10:00:00+00:00");
        assert_eq!(entries[3].title.as_deref(), Some("beta moved"));
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
    assert!(entries.iter().all(|e| e.meeting.duration_minutes == 30));
    // The occurrence it replaces, 09:30 summer time.
    assert_eq!(entries[1].id, "dst-series/2026-03-30T07:30:00+00:00");
}

#[test]
fn an_event_without_a_uid_gets_an_id_from_what_it_is() {
    let text = feed(
        "BEGIN:VEVENT\r
SUMMARY:Meeting\r
DTSTART:20260303T090000Z\r
DTEND:20260303T100000Z\r
END:VEVENT\r
",
    );
    let first = parse_events(&text, &week(), Tz::UTC).unwrap();
    let again = parse_events(&text, &week(), Tz::UTC).unwrap();

    assert!(first[0].id.starts_with("h:"));
    assert_eq!(first[0].id, again[0].id);
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

/// A zone the feed does not define and IANA does not know is read like a time without a zone,
/// in the computer's zone, and said so once per zone.
#[test]
fn an_unknown_zone_is_read_as_the_computers_time_with_a_warning() {
    let text = feed(
        "BEGIN:VEVENT
UID:unknown-zone-review
DTSTART;TZID=Unknown/ReviewZone:20260302T090000
DTEND;TZID=Unknown/ReviewZone:20260302T100000
END:VEVENT
BEGIN:VEVENT
UID:unknown-zone-other
DTSTART;TZID=Unknown/ReviewZone:20260303T090000
DTEND;TZID=Unknown/ReviewZone:20260303T100000
END:VEVENT
",
    );
    let berlin = Tz::from_str("Europe/Berlin").unwrap();

    let entries = parse_events(&text, &week(), berlin).unwrap();

    assert_eq!(entries[0].timestamp, "2026-03-02T08:00:00+00:00");
    let warnings = crate::diagnostics::entries()
        .into_iter()
        .filter(|log| log.message.contains("Unknown/ReviewZone"))
        .collect::<Vec<_>>();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].message.contains("unknown-zone-review"));
    assert_eq!(warnings[0].level, crate::model::LogLevel::Warn);
}

/// A saved link names the occurrence; it must find it after the organizer moves it.
#[test]
fn an_occurrence_keeps_its_id_when_it_is_moved() {
    for (zone, rid) in [
        ("", "RECURRENCE-ID:20260303T080000Z"),
        (
            ";TZID=Europe/Berlin",
            "RECURRENCE-ID;TZID=Europe/Berlin:20260303T090000",
        ),
    ] {
        let series = format!(
            "BEGIN:VEVENT
UID:series
DTSTART{zone}:{start}
DTEND{zone}:{end}
RRULE:FREQ=DAILY;COUNT=3
END:VEVENT
",
            start = if zone.is_empty() {
                "20260302T080000Z"
            } else {
                "20260302T090000"
            },
            end = if zone.is_empty() {
                "20260302T083000Z"
            } else {
                "20260302T093000"
            },
        );
        let id_of_the_3rd = |text: &str| {
            parse_events(text, &week(), Tz::UTC)
                .unwrap()
                .into_iter()
                .find(|entry| {
                    entry.title.as_deref() == Some("Moved")
                        || entry.timestamp.as_str().starts_with("2026-03-03")
                })
                .map(|entry| entry.id)
                .unwrap()
        };

        let before = id_of_the_3rd(&feed(&series));
        let after = id_of_the_3rd(&feed(&format!(
            "{series}BEGIN:VEVENT
UID:series
{rid}
SUMMARY:Moved
DTSTART:20260305T100000Z
DTEND:20260305T110000Z
END:VEVENT
"
        )));

        assert_eq!(before, "series/2026-03-03T08:00:00+00:00", "{zone}");
        assert_eq!(after, before, "{zone}");
    }
}

fn busy_status_of(properties: &str) -> Option<BusyStatus> {
    let text = feed(&format!(
        "BEGIN:VEVENT\r
UID:status\r
SUMMARY:Meeting\r
DTSTART:20260303T090000Z\r
DTEND:20260303T100000Z\r
{properties}END:VEVENT\r
"
    ));
    let entries = parse_events(&text, &week(), Tz::UTC).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].meeting.meeting_weight,
        MeetingWeight::of(entries[0].meeting.busy_status)
    );
    entries[0].meeting.busy_status
}

#[test]
fn reads_each_microsoft_busy_status() {
    for (value, expected) in [
        ("BUSY", BusyStatus::Busy),
        ("TENTATIVE", BusyStatus::Tentative),
        ("FREE", BusyStatus::Free),
        ("OOF", BusyStatus::OutOfOffice),
        ("WORKINGELSEWHERE", BusyStatus::WorkingElsewhere),
        ("oof", BusyStatus::OutOfOffice),
    ] {
        assert_eq!(
            busy_status_of(&format!("X-MICROSOFT-CDO-BUSYSTATUS:{value}\r\n")),
            Some(expected),
            "{value}"
        );
    }
}

#[test]
fn falls_back_to_status_then_transparency() {
    assert_eq!(
        busy_status_of("STATUS:TENTATIVE\r\n"),
        Some(BusyStatus::Tentative)
    );
    assert_eq!(
        busy_status_of("TRANSP:TRANSPARENT\r\n"),
        Some(BusyStatus::Free)
    );
    assert_eq!(busy_status_of("TRANSP:OPAQUE\r\n"), Some(BusyStatus::Busy));
    assert_eq!(busy_status_of("STATUS:CONFIRMED\r\n"), None);
    assert_eq!(busy_status_of(""), None);
}

#[test]
fn the_microsoft_status_wins_over_status_and_transparency() {
    assert_eq!(
        busy_status_of("STATUS:TENTATIVE\r\nTRANSP:OPAQUE\r\nX-MICROSOFT-CDO-BUSYSTATUS:OOF\r\n"),
        Some(BusyStatus::OutOfOffice)
    );
    assert_eq!(
        busy_status_of(
            "STATUS:TENTATIVE\r\nTRANSP:TRANSPARENT\r\nX-MICROSOFT-CDO-BUSYSTATUS:BUSY\r\n"
        ),
        Some(BusyStatus::Busy)
    );
    assert_eq!(
        busy_status_of("X-MICROSOFT-CDO-BUSYSTATUS:SOMETHING\r\nTRANSP:TRANSPARENT\r\n"),
        Some(BusyStatus::Free)
    );
}

#[test]
fn a_transparent_event_is_free_even_when_tentative() {
    // RFC 5545: TRANSP decides whether an event blocks time; STATUS only says how sure it is.
    assert_eq!(
        busy_status_of("STATUS:TENTATIVE\r\nTRANSP:TRANSPARENT\r\n"),
        Some(BusyStatus::Free)
    );
    assert_eq!(
        busy_status_of("TRANSP:TRANSPARENT\r\nSTATUS:TENTATIVE\r\n"),
        Some(BusyStatus::Free)
    );
    assert_eq!(
        busy_status_of("STATUS:TENTATIVE\r\nTRANSP:OPAQUE\r\n"),
        Some(BusyStatus::Tentative)
    );
}

#[test]
fn an_override_brings_its_own_busy_status() {
    let text = feed(
        "BEGIN:VEVENT
UID:series
SUMMARY:Weekly
X-MICROSOFT-CDO-BUSYSTATUS:BUSY
DTSTART:20260302T080000Z
DTEND:20260302T083000Z
RRULE:FREQ=DAILY;COUNT=3
END:VEVENT
BEGIN:VEVENT
UID:series
RECURRENCE-ID:20260303T080000Z
SUMMARY:Weekly (maybe)
X-MICROSOFT-CDO-BUSYSTATUS:TENTATIVE
DTSTART:20260303T080000Z
DTEND:20260303T083000Z
END:VEVENT
BEGIN:VEVENT
UID:series
RECURRENCE-ID:20260304T080000Z
SUMMARY:Weekly (moved)
DTSTART:20260304T120000Z
DTEND:20260304T123000Z
END:VEVENT
",
    );
    let entries = parse_events(&text, &week(), Tz::UTC).unwrap();
    let statuses = entries
        .iter()
        .map(|entry| (entry.title.as_deref().unwrap(), entry.meeting.busy_status))
        .collect::<Vec<_>>();
    assert_eq!(
        statuses,
        vec![
            ("Weekly", Some(BusyStatus::Busy)),
            ("Weekly (maybe)", Some(BusyStatus::Tentative)),
            // An override replaces the whole occurrence; without its own status it has none.
            ("Weekly (moved)", None),
        ]
    );
}

fn single_event(properties: &str) -> crate::model::CalendarEntry {
    let text = feed(&format!(
        "BEGIN:VEVENT\r
UID:meeting\r
SUMMARY:Meeting\r
DTSTART:20260303T090000Z\r
DTEND:20260303T100000Z\r
{properties}END:VEVENT\r
"
    ));
    let mut entries = parse_events(&text, &week(), Tz::UTC).unwrap();
    assert_eq!(entries.len(), 1);
    entries.remove(0)
}

#[test]
fn unescapes_and_unfolds_the_description() {
    let entry = single_event(
        "DESCRIPTION:Agenda:\\n1. Budget\\, staffing\\; risks\\n\\nPath C:\\\\temp\\n\r
  continued on a folded line\r
",
    );
    assert_eq!(
        entry.meeting.description.as_deref(),
        Some("Agenda:\n\n1. Budget, staffing; risks\n\nPath C:\\temp\ncontinued on a folded line")
    );
    assert_eq!(entry.meeting.join_url, None);
}

#[test]
fn reads_the_html_description_outlook_adds() {
    use crate::model::{DescriptionBlock, TextSegment};

    let entry = single_event(
        "DESCRIPTION:Plain\\, as text\\n\r
X-ALT-DESC;FMTTYPE=text/html:<html><body><p style=\"margin:0\\;color:red\">Hello\r
  <b>team</b>\\,</p><ul><li>One</li></ul></body></html>\\n\r
",
    );
    assert_eq!(
        entry.meeting.description.as_deref(),
        Some("Hello team,\n\n• One")
    );
    assert_eq!(
        entry.meeting.description_blocks.first(),
        Some(&DescriptionBlock::Paragraph {
            segments: vec![
                TextSegment::text("Hello "),
                TextSegment::Text {
                    text: "team".to_string(),
                    strong: true,
                    emphasis: false,
                },
                TextSegment::text(","),
            ]
        })
    );
}

#[test]
fn reads_the_join_link_and_drops_the_invitation_from_the_description() {
    let rule = "_".repeat(40);
    let entry = single_event(&format!(
        "DESCRIPTION:Weekly sync\\n\\n{rule}\\nMicrosoft Teams-Besprechung\\nJetzt an der Besprechung teilnehmen<https://eur03.safelinks.protection.outlook.com/?url=https%3A%2F%2Fteams.microsoft.com%2Fl%2Fmeetup-join%2F19%253ameeting_x%2540thread.v2%2F0&data=05%7C02&reserved=0>\\n{rule}\\n\r
"
    ));
    assert_eq!(entry.meeting.description.as_deref(), Some("Weekly sync"));
    assert_eq!(
        entry.meeting.join_url.as_deref(),
        Some("https://teams.microsoft.com/l/meetup-join/19%3ameeting_x%40thread.v2/0")
    );
}

#[test]
fn an_explicit_join_property_wins_over_the_description() {
    let entry = single_event(
        "DESCRIPTION:Fallback https://zoom.us/j/1\r
X-GOOGLE-CONFERENCE:https://meet.google.com/abc-defg-hij\r
X-MICROSOFT-SKYPETEAMSMEETINGURL:https://teams.microsoft.com/l/meetup-join/explicit;part\r
",
    );
    assert_eq!(
        entry.meeting.join_url.as_deref(),
        Some("https://teams.microsoft.com/l/meetup-join/explicit;part")
    );
    assert_eq!(
        single_event("X-GOOGLE-CONFERENCE:https://meet.google.com/abc-defg-hij\r\n")
            .meeting
            .join_url
            .as_deref(),
        Some("https://meet.google.com/abc-defg-hij")
    );
}

#[test]
fn the_url_property_counts_only_when_it_is_a_meeting_link() {
    assert_eq!(
        single_event("URL:https://zoom.us/j/123\r\n")
            .meeting
            .join_url
            .as_deref(),
        Some("https://zoom.us/j/123")
    );
    assert_eq!(
        single_event("URL:https://example.org/event/1\r\n")
            .meeting
            .join_url,
        None
    );
}

#[test]
fn an_override_brings_its_own_description() {
    let text = feed(
        "BEGIN:VEVENT
UID:series
SUMMARY:Daily
DESCRIPTION:Series notes https://meet.google.com/aaa-bbbb-ccc
DTSTART:20260302T080000Z
DTEND:20260302T083000Z
RRULE:FREQ=DAILY;COUNT=2
END:VEVENT
BEGIN:VEVENT
UID:series
RECURRENCE-ID:20260303T080000Z
SUMMARY:Daily (changed)
DESCRIPTION:Only today
DTSTART:20260303T080000Z
DTEND:20260303T083000Z
END:VEVENT
",
    );
    let entries = parse_events(&text, &week(), Tz::UTC).unwrap();
    let meetings = entries
        .iter()
        .map(|entry| {
            (
                entry.meeting.description.as_deref(),
                entry.meeting.join_url.as_deref(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        meetings,
        vec![
            (
                Some("Series notes https://meet.google.com/aaa-bbbb-ccc"),
                Some("https://meet.google.com/aaa-bbbb-ccc")
            ),
            (Some("Only today"), None),
        ]
    );
}

/// A series that runs through more occurrences than its budget before the range is cut and
/// reported, and the series after it are still expanded.
#[test]
fn one_long_series_cannot_use_up_the_others_budget() {
    let text = feed(
        "BEGIN:VEVENT
UID:every-minute
SUMMARY:Every minute
DTSTART:20250901T000000Z
DTEND:20250901T000030Z
RRULE:FREQ=MINUTELY;UNTIL=20260310T000000Z
END:VEVENT
BEGIN:VEVENT
UID:standup
SUMMARY:Standup
DTSTART:20260302T090000Z
DTEND:20260302T091500Z
RRULE:FREQ=DAILY;COUNT=5
END:VEVENT
",
    );

    let entries = parse_events(&text, &week(), Tz::UTC).unwrap();

    let standups = entries
        .iter()
        .filter(|entry| entry.title.as_deref() == Some("Standup"))
        .count();
    assert_eq!(standups, 5);
    assert!(crate::diagnostics::entries().iter().any(|log| {
        log.message.contains("every-minute") && log.message.contains("may be missing")
    }));
    assert!(
        !crate::diagnostics::entries()
            .iter()
            .any(|log| log.message.contains("UID standup") && log.message.contains("Stopped"))
    );
}

/// The startup check only sees whether the feed can be read; the collection reads it.
#[tokio::test]
async fn the_check_opens_the_feed_without_reading_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("export.ics");
    std::fs::write(&path, OUTLOOK_FEED).unwrap();

    check_feed(&path.display().to_string()).await.unwrap();
    let missing = check_feed(&dir.path().join("gone.ics").display().to_string())
        .await
        .unwrap_err();
    assert!(format!("{missing:#}").contains("gone.ics"));
    assert!(
        check_feed(&dir.path().display().to_string()).await.is_err(),
        "a folder is no feed"
    );

    let unreachable = check_feed("https://127.0.0.1:9/secret-token/calendar.ics")
        .await
        .unwrap_err();
    let message = format!("{unreachable:#}");
    assert!(!message.contains("secret-token"), "{message}");
    assert!(message.contains("127.0.0.1"), "{message}");
    assert_eq!(
        crate::provider::source_failure::problem_of(&unreachable),
        Some(crate::model::ProviderProblem::Unreachable)
    );
}

/// A feed link the host refuses or no longer serves keeps its status as a typed cause.
#[tokio::test]
async fn a_feed_the_host_refuses_names_its_problem() {
    use crate::provider::remote_api::test_server::{Reply, TestServer};
    for (status, problem) in [
        (401, Some(crate::model::ProviderProblem::Refused)),
        (404, None),
    ] {
        let server = TestServer::start(move |_| Reply::json(status, "{}"));
        // The test server speaks plain HTTP, which `feed_location` refuses: ask it directly.
        let error = super::request_feed(server.url.parse().unwrap())
            .await
            .err()
            .unwrap();
        let failure = error
            .downcast_ref::<crate::provider::remote_error::RemoteFailure>()
            .unwrap();
        assert_eq!(failure.error.problem(), problem, "{status}");
        assert_eq!(failure.service, "127.0.0.1");
    }
}

/// `X-ALT-DESC` says what it is: HTML when it is declared `text/html`, whatever tags it holds,
/// and text when declared so.
#[test]
fn the_declared_format_of_the_html_description_wins() {
    use crate::model::{DescriptionBlock, TextSegment};

    let heading = single_event(
        "DESCRIPTION:Agenda\r
X-ALT-DESC;FMTTYPE=text/html:<h2>Agenda</h2>\r
",
    );
    assert_eq!(
        heading.meeting.description_blocks,
        [DescriptionBlock::Heading {
            segments: vec![TextSegment::text("Agenda")]
        }]
    );

    let declared_text = single_event(
        "DESCRIPTION:Plain\r
X-ALT-DESC;FMTTYPE=text/plain:<p>Not used</p>\r
",
    );
    assert_eq!(declared_text.meeting.description.as_deref(), Some("Plain"));
}

/// A UTC series read on a computer in Berlin keeps its UTC times, and a moved occurrence
/// replaces exactly the one it names.
#[test]
fn a_utc_series_keeps_its_times_in_any_computer_zone() {
    let series = "BEGIN:VEVENT
UID:utc-series
SUMMARY:Standup
DTSTART:20260302T080000Z
DTEND:20260302T081500Z
RRULE:FREQ=DAILY;COUNT=5
END:VEVENT
";
    let berlin = Tz::from_str("Europe/Berlin").unwrap();
    let times = |text: &str| {
        parse_events(text, &week(), berlin)
            .unwrap()
            .into_iter()
            .map(|entry| (entry.timestamp, entry.id))
            .collect::<Vec<_>>()
    };

    let plain = times(&feed(series));
    assert_eq!(
        plain.iter().map(|(at, _)| at.as_str()).collect::<Vec<_>>(),
        [
            "2026-03-02T08:00:00+00:00",
            "2026-03-03T08:00:00+00:00",
            "2026-03-04T08:00:00+00:00",
            "2026-03-05T08:00:00+00:00",
            "2026-03-06T08:00:00+00:00",
        ]
    );

    let moved = times(&feed(&format!(
        "{series}BEGIN:VEVENT
UID:utc-series
RECURRENCE-ID:20260303T080000Z
SUMMARY:Standup (moved)
DTSTART:20260303T100000Z
DTEND:20260303T101500Z
END:VEVENT
"
    )));
    assert_eq!(moved.len(), 5, "{moved:?}");
    assert!(
        moved
            .iter()
            .any(|(at, id)| at.as_str() == "2026-03-03T10:00:00+00:00"
                && id == "utc-series/2026-03-03T08:00:00+00:00")
    );
    assert!(
        !moved
            .iter()
            .any(|(at, _)| at.as_str() == "2026-03-03T08:00:00+00:00")
    );
}

/// "This and future" moves every later occurrence; each keeps the id of when it was planned,
/// whatever range is collected.
#[test]
fn a_this_and_future_move_keeps_one_id_per_planned_occurrence() {
    let text = feed(
        "BEGIN:VEVENT
UID:daily
SUMMARY:Daily
DTSTART;TZID=UTC:20260302T080000
DTEND;TZID=UTC:20260302T081500
RRULE:FREQ=DAILY;COUNT=5
END:VEVENT
BEGIN:VEVENT
UID:daily
RECURRENCE-ID;TZID=UTC;RANGE=THISANDFUTURE:20260303T080000
SUMMARY:Daily (later)
DTSTART;TZID=UTC:20260303T100000
DTEND;TZID=UTC:20260303T101500
END:VEVENT
",
    );
    let ids = |options: &CollectOptions| {
        parse_events(&text, options, Tz::UTC)
            .unwrap()
            .into_iter()
            .map(|entry| (entry.timestamp, entry.id))
            .collect::<Vec<_>>()
    };

    let all = ids(&week());
    assert_eq!(
        all.iter().map(|(_, id)| id.as_str()).collect::<Vec<_>>(),
        [
            "daily/2026-03-02T08:00:00+00:00",
            "daily/2026-03-03T08:00:00+00:00",
            "daily/2026-03-04T08:00:00+00:00",
            "daily/2026-03-05T08:00:00+00:00",
            "daily/2026-03-06T08:00:00+00:00",
        ]
    );
    assert_eq!(all[2].0, "2026-03-04T10:00:00+00:00");
    let from_wednesday = ids(&CollectOptions {
        start: Utc.with_ymd_and_hms(2026, 3, 4, 0, 0, 0).unwrap(),
        end: week().end,
    });
    assert_eq!(from_wednesday[0], all[2]);
}

/// Exclusions the series never reaches cost none of the budget, and a series that ends exactly
/// at its share is complete.
#[test]
fn only_dates_a_series_runs_through_count_against_the_budget() {
    let mut events = String::new();
    for series in 0..4 {
        let exdates = (0..25_000)
            .map(|minute| {
                (Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap()
                    + chrono::Duration::minutes(minute))
                .format("%Y%m%dT%H%M%SZ")
                .to_string()
            })
            .collect::<Vec<_>>()
            .join(",");
        events.push_str(&format!(
            "BEGIN:VEVENT
UID:minutes-{series}
DTSTART:20260201T000000Z
DTEND:20260201T000030Z
RRULE:FREQ=MINUTELY;COUNT=25000
EXDATE:{exdates}
END:VEVENT
"
        ));
    }
    events.push_str(
        "BEGIN:VEVENT
UID:standup-budget
SUMMARY:Standup
DTSTART:20260302T090000Z
DTEND:20260302T091500Z
RRULE:FREQ=DAILY;COUNT=5
END:VEVENT
BEGIN:VEVENT
UID:exactly-at-the-limit
DTSTART:20200101T000000Z
DTEND:20200101T000030Z
RRULE:FREQ=MINUTELY;COUNT=50000
END:VEVENT
",
    );

    let entries = parse_events(&feed(&events), &week(), Tz::UTC).unwrap();

    assert_eq!(
        entries
            .iter()
            .filter(|entry| entry.title.as_deref() == Some("Standup"))
            .count(),
        5
    );
    assert!(!crate::diagnostics::entries().iter().any(|log| {
        (log.message.contains("minutes-") || log.message.contains("exactly-at-the-limit"))
            && log.message.contains("may be missing")
    }));
}

/// Each series is expanded without walking the others: 8,000 short series stay far below a
/// second (walking the whole feed per series took about 13 s).
#[test]
fn many_short_series_expand_in_linear_time() {
    let mut events = String::new();
    for zone in 0..10 {
        events.push_str(&format!(
            "BEGIN:VTIMEZONE\nTZID:Zone {zone}\nX-LIC-LOCATION:Europe/Berlin\nEND:VTIMEZONE\n"
        ));
    }
    for series in 0..8_000 {
        events.push_str(&format!(
            "BEGIN:VEVENT\nUID:short-{series}\nDTSTART;TZID=Zone {zone}:20260303T090000\nDTEND;TZID=Zone {zone}:20260303T093000\nRRULE:FREQ=DAILY;COUNT=1\nEND:VEVENT\n",
            zone = series % 10
        ));
    }
    let text = feed(&events);

    let started = std::time::Instant::now();
    let entries = parse_events(&text, &week(), Tz::UTC).unwrap();

    assert_eq!(entries.len(), 8_000);
    assert!(
        started.elapsed() < std::time::Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );
}

/// A "this and future" move and a later single move of one occurrence: five meetings for five
/// planned dates, the moved one under its planned id in any range.
#[test]
fn a_single_move_after_a_this_and_future_move_replaces_its_occurrence() {
    let text = feed(
        "BEGIN:VEVENT
UID:daily
SUMMARY:base
DTSTART:20260302T080000Z
DTEND:20260302T081500Z
RRULE:FREQ=DAILY;COUNT=5
END:VEVENT
BEGIN:VEVENT
UID:daily
SUMMARY:range
RECURRENCE-ID;RANGE=THISANDFUTURE:20260303T080000Z
DTSTART:20260303T100000Z
DTEND:20260303T101500Z
END:VEVENT
BEGIN:VEVENT
UID:daily
SUMMARY:moved
RECURRENCE-ID:20260305T080000Z
DTSTART:20260307T120000Z
DTEND:20260307T121500Z
END:VEVENT
",
    );
    let berlin = Tz::from_str("Europe/Berlin").unwrap();
    let rows = |options: &CollectOptions| {
        parse_events(&text, options, berlin)
            .unwrap()
            .into_iter()
            .map(|entry| (entry.title.unwrap(), entry.timestamp.to_string(), entry.id))
            .collect::<Vec<_>>()
    };

    let all = rows(&week());
    assert_eq!(all.len(), 5, "{all:?}");
    let moved = (
        "moved".to_string(),
        "2026-03-07T12:00:00+00:00".to_string(),
        "daily/2026-03-05T08:00:00+00:00".to_string(),
    );
    assert!(all.contains(&moved), "{all:?}");
    let ids = all
        .iter()
        .map(|(_, _, id)| id)
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(ids.len(), 5, "{all:?}");
    let saturday = rows(&CollectOptions {
        start: Utc.with_ymd_and_hms(2026, 3, 7, 0, 0, 0).unwrap(),
        end: week().end,
    });
    assert_eq!(saturday, [moved]);
}

/// The budget counts every date a series runs through, the ones its exclusions remove too, and
/// no occurrence past a series' share is shown.
#[test]
fn excluded_dates_inside_a_series_count_and_a_cut_series_stops_at_its_share() {
    let minute = |n: i64| {
        (Utc.with_ymd_and_hms(2026, 2, 1, 0, 0, 0).unwrap() + chrono::Duration::minutes(n))
            .format("%Y%m%dT%H%M%SZ")
            .to_string()
    };
    let mut events = String::new();
    for series in 0..4 {
        // 50,000 minutes, every other one excluded: 25,000 shown, 50,000 run through.
        let exdates = (0..25_000)
            .map(|n| minute(2 * n + 1))
            .collect::<Vec<_>>()
            .join(",");
        events.push_str(&format!(
            "BEGIN:VEVENT
UID:half-excluded-{series}
DTSTART:20260201T000000Z
DTEND:20260201T000030Z
RRULE:FREQ=MINUTELY;COUNT=50000
EXDATE:{exdates}
END:VEVENT
"
        ));
    }
    events.push_str(
        "BEGIN:VEVENT
UID:after-the-budget
SUMMARY:Too late
DTSTART:20260302T090000Z
DTEND:20260302T091500Z
RRULE:FREQ=DAILY;COUNT=5
END:VEVENT
",
    );
    let all_time = CollectOptions {
        start: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
        end: Utc.with_ymd_and_hms(2026, 4, 1, 0, 0, 0).unwrap(),
    };

    let entries = parse_events(&feed(&events), &all_time, Tz::UTC).unwrap();

    assert_eq!(
        entries.len(),
        100_000,
        "the four series, nothing past the budget"
    );
    assert!(crate::diagnostics::entries().iter().any(|log| {
        log.message
            .contains("Stopped after 200000 occurrences in all")
    }));

    let mut capped = String::new();
    for series in 0..4 {
        capped.push_str(&format!(
            "BEGIN:VEVENT
UID:capped-{series}
DTSTART:20260201T000000Z
DTEND:20260201T000030Z
RRULE:FREQ=SECONDLY;COUNT=60000
END:VEVENT
"
        ));
    }
    let entries = parse_events(&feed(&capped), &all_time, Tz::UTC).unwrap();
    assert_eq!(
        entries.len(),
        200_000,
        "four shares of 50,000, no lookahead"
    );
}

/// A stopped collection stops its expansion between series.
#[test]
fn a_stopped_expansion_ends_between_series() {
    use std::sync::atomic::AtomicBool;

    let text = feed(
        "BEGIN:VEVENT
UID:stopped
DTSTART:20260302T090000Z
DTEND:20260302T091500Z
RRULE:FREQ=DAILY;COUNT=5
END:VEVENT
",
    );

    let err =
        super::parse_events_until(&text, &week(), Tz::UTC, &AtomicBool::new(true)).unwrap_err();
    assert!(err.is::<crate::platform::blocking::Stopped>(), "{err:#}");
}

/// A daily series of five with a "this and future" move from March 3 and the given extra
/// components, as `(title, start, id)` in the first week of March, read in Berlin.
fn moved_series(master_extra: &str, range: &str, overrides: &str) -> Vec<(String, String, String)> {
    let text = feed(&format!(
        "BEGIN:VEVENT
UID:daily
SUMMARY:base
DTSTART:20260302T080000Z
DTEND:20260302T081500Z
RRULE:FREQ=DAILY;COUNT=5
{master_extra}END:VEVENT
BEGIN:VEVENT
UID:daily
SUMMARY:range
RECURRENCE-ID;RANGE=THISANDFUTURE:20260303T080000Z
{range}END:VEVENT
{overrides}"
    ));
    let options = CollectOptions {
        start: week().start,
        end: Utc.with_ymd_and_hms(2026, 3, 10, 0, 0, 0).unwrap(),
    };
    parse_events(&text, &options, Tz::from_str("Europe/Berlin").unwrap())
        .unwrap()
        .into_iter()
        .map(|entry| {
            (
                entry.title.unwrap(),
                entry.timestamp.as_str()[..16].to_string(),
                entry.id,
            )
        })
        .collect()
}

fn single_move(rid: &str, start: &str, title: &str) -> String {
    format!(
        "BEGIN:VEVENT
UID:daily
SUMMARY:{title}
RECURRENCE-ID:{rid}
DTSTART:{start}
DTEND:{start}
END:VEVENT
"
    )
}

fn row(title: &str, start: &str, planned: &str) -> (String, String, String) {
    (
        title.to_string(),
        start.to_string(),
        format!("daily/{planned}:00+00:00"),
    )
}

#[test]
fn a_range_move_by_a_day_and_a_single_move_keep_one_meeting_per_planned_date() {
    let rows = moved_series(
        "",
        "DTSTART:20260304T080000Z\nDTEND:20260304T081500Z\n",
        &single_move("20260305T080000Z", "20260307T120000Z", "moved"),
    );
    assert_eq!(
        rows,
        [
            row("base", "2026-03-02T08:00", "2026-03-02T08:00"),
            row("range", "2026-03-04T08:00", "2026-03-03T08:00"),
            row("range", "2026-03-05T08:00", "2026-03-04T08:00"),
            row("range", "2026-03-07T08:00", "2026-03-06T08:00"),
            row("moved", "2026-03-07T12:00", "2026-03-05T08:00"),
        ]
    );
}

/// Extra dates are occurrences of their own: moving the master's 08:00 leaves the override's
/// 10:00 in place.
#[test]
fn moving_one_extra_date_leaves_the_other() {
    let rows = moved_series(
        "RDATE:20260307T080000Z\n",
        "DTSTART:20260303T100000Z\nDTEND:20260303T101500Z\nRDATE:20260307T100000Z\n",
        &single_move("20260307T080000Z", "20260308T120000Z", "moved"),
    );
    assert_eq!(rows.len(), 7, "{rows:?}");
    assert!(rows.contains(&row("range", "2026-03-07T10:00", "2026-03-07T10:00")));
    assert!(rows.contains(&row("moved", "2026-03-08T12:00", "2026-03-07T08:00")));
}

/// Two single moves after a range move each replace their own planned occurrence.
#[test]
fn two_single_moves_after_a_range_move_replace_their_own_occurrences() {
    let rows = moved_series(
        "",
        "DTSTART:20260303T100000Z\nDTEND:20260303T101500Z\n",
        &format!(
            "{}{}",
            single_move("20260305T080000Z", "20260307T120000Z", "moved"),
            single_move("20260304T080000Z", "20260305T080000Z", "moved-another")
        ),
    );
    assert_eq!(
        rows,
        [
            row("base", "2026-03-02T08:00", "2026-03-02T08:00"),
            row("range", "2026-03-03T10:00", "2026-03-03T08:00"),
            row("moved-another", "2026-03-05T08:00", "2026-03-04T08:00"),
            row("range", "2026-03-06T10:00", "2026-03-06T08:00"),
            row("moved", "2026-03-07T12:00", "2026-03-05T08:00"),
        ]
    );
}

/// An override naming no planned occurrence (also not one's moved time) is a meeting of its
/// own, named by its RECURRENCE-ID; the planned occurrences all stay.
#[test]
fn an_override_that_names_no_planned_occurrence_is_an_extra_meeting() {
    for rid in ["20260305T160000Z", "20260305T100000Z"] {
        let rows = moved_series(
            "",
            "DTSTART:20260303T100000Z\nDTEND:20260303T101500Z\n",
            &single_move(rid, "20260307T120000Z", "moved"),
        );
        assert_eq!(rows.len(), 6, "{rid}: {rows:?}");
        assert!(rows.contains(&row("range", "2026-03-05T10:00", "2026-03-05T08:00")));
        let planned = format!(
            "{}-{}-{}T{}:{}",
            &rid[..4],
            &rid[4..6],
            &rid[6..8],
            &rid[9..11],
            &rid[11..13]
        );
        assert!(
            rows.contains(&row("moved", "2026-03-07T12:00", &planned)),
            "{rid}: {rows:?}"
        );
    }
}

/// A feed of many single events is stopped between batches too.
#[test]
fn a_stopped_expansion_of_many_single_events_ends_early() {
    use std::sync::atomic::AtomicBool;

    let mut events = String::new();
    for event in 0..3_000 {
        events.push_str(&format!(
            "BEGIN:VEVENT\nUID:single-{event}\nDTSTART:20260303T090000Z\nDTEND:20260303T093000Z\nEND:VEVENT\n"
        ));
    }

    let err = super::parse_events_until(&feed(&events), &week(), Tz::UTC, &AtomicBool::new(true))
        .unwrap_err();
    assert!(err.is::<crate::platform::blocking::Stopped>(), "{err:#}");
}

/// Moving an 08:00 the series never planned, beside the override's own 10:00: nothing planned
/// is replaced, and the move is a meeting of its own.
#[test]
fn moving_an_extra_date_the_series_does_not_have_adds_a_meeting() {
    let rows = moved_series(
        "",
        "DTSTART:20260303T100000Z\nDTEND:20260303T101500Z\nRDATE:20260307T100000Z\n",
        &single_move("20260307T080000Z", "20260308T120000Z", "moved"),
    );
    assert_eq!(rows.len(), 7, "{rows:?}");
    assert!(rows.contains(&row("range", "2026-03-07T10:00", "2026-03-07T10:00")));
    assert!(rows.contains(&row("moved", "2026-03-08T12:00", "2026-03-07T08:00")));
}

/// A feed's entries from February to April as `(title, start, id)`, after checking that every
/// day of it, collected alone, shows the same entries under the same ids.
fn in_any_range(events: &str) -> Vec<(String, String, String)> {
    let text = feed(events);
    let berlin = Tz::from_str("Europe/Berlin").unwrap();
    let rows = |options: &CollectOptions| {
        // Exact ends: an entry has its length in whole minutes only.
        let mut rows = super::entries_with_ends(&text, options, berlin, &AtomicBool::new(false))
            .unwrap()
            .into_iter()
            .map(|(entry, end)| {
                let start = chrono::DateTime::parse_from_rfc3339(entry.timestamp.as_str())
                    .unwrap()
                    .to_utc();
                (start, end, entry.title.unwrap_or_default(), entry.id)
            })
            .collect::<Vec<_>>();
        rows.sort();
        rows
    };
    let all = rows(&CollectOptions {
        start: Utc.with_ymd_and_hms(2026, 2, 1, 0, 0, 0).unwrap(),
        end: Utc.with_ymd_and_hms(2026, 5, 1, 0, 0, 0).unwrap(),
    });
    let ids = all
        .iter()
        .map(|(_, _, _, id)| id)
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(ids.len(), all.len(), "one id per entry: {all:?}");
    // Every day an entry starts on, runs through or ends on.
    let days = all
        .iter()
        .flat_map(|(start, end, ..)| {
            let first = start.date_naive().and_hms_opt(0, 0, 0).unwrap().and_utc();
            (0..)
                .map(move |day| first + chrono::Duration::days(day))
                .take_while(move |day| day == &first || day < end)
        })
        .collect::<BTreeSet<_>>();
    for day in days {
        let options = CollectOptions {
            start: day,
            end: day + chrono::Duration::days(1),
        };
        let expected = all
            .iter()
            .filter(|(start, end, ..)| options.overlaps(*start, *end))
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(rows(&options), expected, "{day}");
    }
    all.into_iter()
        .map(|(start, _, title, id)| (title, start.format("%m-%dT%H:%M").to_string(), id))
        .collect()
}

/// `(title, start, id)` rows, ids as `s/<planned>`.
fn rows(expected: &[(&str, &str, &str)]) -> Vec<(String, String, String)> {
    expected
        .iter()
        .map(|(title, start, planned)| {
            (
                title.to_string(),
                start.to_string(),
                format!("s/2026-{planned}:00+00:00"),
            )
        })
        .collect()
}

const DAILY_FIVE: &str = "BEGIN:VEVENT
UID:s
SUMMARY:base
DTSTART:20260302T080000Z
DTEND:20260302T083000Z
RRULE:FREQ=DAILY;COUNT=5
";

fn override_of(rid: &str, rest: &str) -> String {
    format!("BEGIN:VEVENT\nUID:s\nSUMMARY:move\nRECURRENCE-ID{rid}\n{rest}END:VEVENT\n")
}

#[test]
fn an_rdate_series_takes_its_moves_and_cancellations() {
    let series = "BEGIN:VEVENT
UID:s
SUMMARY:base
DTSTART:20260302T080000Z
DTEND:20260302T083000Z
RDATE:20260304T080000Z,20260305T080000Z
END:VEVENT
";
    let moved = in_any_range(&format!(
        "{series}{}",
        override_of(
            ":20260304T080000Z",
            "DTSTART:20260308T120000Z\nDURATION:PT45M\n"
        )
    ));
    assert_eq!(
        moved,
        rows(&[
            ("base", "03-02T08:00", "03-02T08:00"),
            ("base", "03-05T08:00", "03-05T08:00"),
            ("move", "03-08T12:00", "03-04T08:00"),
        ])
    );
    let cancelled = in_any_range(&format!(
        "{series}{}",
        override_of(":20260304T080000Z", "STATUS:CANCELLED\n")
    ));
    assert_eq!(cancelled.len(), 2, "{cancelled:?}");
    let range = in_any_range(&format!(
        "{series}{}",
        override_of(
            ";RANGE=THISANDFUTURE:20260304T080000Z",
            "DTSTART:20260304T100000Z\nDURATION:PT45M\n"
        )
    ));
    assert_eq!(
        range,
        rows(&[
            ("base", "03-02T08:00", "03-02T08:00"),
            ("move", "03-04T10:00", "03-04T08:00"),
            ("move", "03-05T10:00", "03-05T08:00"),
        ])
    );
}

/// A date the rule and an `RDATE` both name is one occurrence.
#[test]
fn a_date_the_rule_and_an_extra_date_both_name_is_one_occurrence() {
    let series = format!("{DAILY_FIVE}RDATE:20260304T080000Z\nEND:VEVENT\n");
    assert_eq!(in_any_range(&series).len(), 5);
    let moved = in_any_range(&format!(
        "{series}{}",
        override_of(
            ":20260304T080000Z",
            "DTSTART:20260308T120000Z\nDURATION:PT45M\n"
        )
    ));
    assert_eq!(moved.len(), 5, "{moved:?}");
    assert!(moved.contains(&rows(&[("move", "03-08T12:00", "03-04T08:00")])[0]));
    let cancelled = in_any_range(&format!(
        "{series}{}",
        override_of(":20260304T080000Z", "STATUS:CANCELLED\n")
    ));
    assert_eq!(cancelled.len(), 4, "{cancelled:?}");
}

/// A "this and future" change applies from the occurrence it names; one that names none is a
/// meeting of its own when active and nothing when cancelled, and changes no other meeting.
#[test]
fn a_range_change_needs_a_planned_occurrence_to_start_from() {
    let series = format!("{DAILY_FIVE}END:VEVENT\n");
    let base = |days: &[u32]| {
        days.iter()
            .map(|day| (format!("03-0{day}T08:00"), format!("03-0{day}T08:00")))
            .collect::<Vec<_>>()
    };
    let unmatched_cancel = in_any_range(&format!(
        "{series}{}",
        override_of(
            ";RANGE=THISANDFUTURE:20260304T100000Z",
            "STATUS:CANCELLED\n"
        )
    ));
    assert_eq!(unmatched_cancel.len(), 5, "{unmatched_cancel:?}");
    let unmatched_move = in_any_range(&format!(
        "{series}{}",
        override_of(
            ";RANGE=THISANDFUTURE:20260303T100000Z",
            "DTSTART:20260303T120000Z\nDURATION:PT45M\n"
        )
    ));
    assert_eq!(unmatched_move.len(), 6, "{unmatched_move:?}");
    assert!(unmatched_move.contains(&rows(&[("move", "03-03T12:00", "03-03T10:00")])[0]));
    for (start, planned) in base(&[2, 3, 4, 5, 6]) {
        assert!(unmatched_move.contains(&rows(&[("base", &start, &planned)])[0]));
    }
    let matched_cancel = in_any_range(&format!(
        "{series}{}",
        override_of(
            ";RANGE=THISANDFUTURE:20260304T080000Z",
            "STATUS:CANCELLED\n"
        )
    ));
    assert_eq!(matched_cancel.len(), 2, "{matched_cancel:?}");
    let after_the_last = in_any_range(&format!(
        "{series}{}",
        override_of(
            ";RANGE=THISANDFUTURE:20260310T080000Z",
            "DTSTART:20260310T120000Z\nDURATION:PT45M\n"
        )
    ));
    assert_eq!(after_the_last.len(), 6, "{after_the_last:?}");
    let excluded_anchor = in_any_range(&format!(
        "{DAILY_FIVE}EXDATE:20260304T080000Z\nEND:VEVENT\n{}",
        override_of(
            ";RANGE=THISANDFUTURE:20260304T080000Z",
            "DTSTART:20260304T100000Z\nDURATION:PT45M\n"
        )
    ));
    assert_eq!(
        excluded_anchor,
        rows(&[
            ("base", "03-02T08:00", "03-02T08:00"),
            ("base", "03-03T08:00", "03-03T08:00"),
            ("move", "03-04T10:00", "03-04T08:00"),
            ("base", "03-05T08:00", "03-05T08:00"),
            ("base", "03-06T08:00", "03-06T08:00"),
        ])
    );
}

/// A change moving meetings back into a range from after it: the occurrences it moves are
/// planned after the range and still found.
#[test]
fn a_backward_range_move_shows_in_the_range_it_moves_into() {
    let rows_found = in_any_range(&format!(
        "BEGIN:VEVENT
UID:s
SUMMARY:weekly
DTSTART:20260302T080000Z
DURATION:PT30M
RRULE:FREQ=WEEKLY;UNTIL=20260331T000000Z
END:VEVENT
{}",
        override_of(
            ";RANGE=THISANDFUTURE:20260316T080000Z",
            "DTSTART:20260306T120000Z\nDURATION:PT45M\n"
        )
    ));
    assert_eq!(
        rows_found,
        rows(&[
            ("weekly", "03-02T08:00", "03-02T08:00"),
            ("move", "03-06T12:00", "03-16T08:00"),
            ("weekly", "03-09T08:00", "03-09T08:00"),
            ("move", "03-13T12:00", "03-23T08:00"),
            ("move", "03-20T12:00", "03-30T08:00"),
        ])
    );

    // Unbounded, collected for March 6 alone.
    let text = feed(&format!(
        "BEGIN:VEVENT
UID:s
SUMMARY:weekly
DTSTART:20260302T080000Z
DURATION:PT30M
RRULE:FREQ=WEEKLY
END:VEVENT
{}",
        override_of(
            ";RANGE=THISANDFUTURE:20260316T080000Z",
            "DTSTART:20260306T120000Z\nDURATION:PT45M\n"
        )
    ));
    for (day, planned) in [
        (6, "2026-03-16T08:00:00+00:00"),
        (13, "2026-03-23T08:00:00+00:00"),
    ] {
        let entries = parse_events(
            &text,
            &CollectOptions {
                start: Utc.with_ymd_and_hms(2026, 3, day, 0, 0, 0).unwrap(),
                end: Utc.with_ymd_and_hms(2026, 3, day + 1, 0, 0, 0).unwrap(),
            },
            Tz::UTC,
        )
        .unwrap();
        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.id.as_str())
                .collect::<Vec<_>>(),
            [format!("s/{planned}")],
            "March {day}"
        );
    }
}

/// Changes on changes: two range changes, a single change before and after a range change,
/// a single change of a moved occurrence, forward moves across the start of a range.
#[test]
fn chains_of_changes_keep_one_meeting_per_planned_occurrence() {
    let series = format!("{DAILY_FIVE}END:VEVENT\n");
    let two_ranges = in_any_range(&format!(
        "{series}{}{}",
        override_of(
            ";RANGE=THISANDFUTURE:20260305T080000Z",
            "DTSTART:20260305T150000Z\nDURATION:PT45M\n"
        ),
        override_of(
            ";RANGE=THISANDFUTURE:20260303T080000Z",
            "DTSTART:20260303T100000Z\nDURATION:PT45M\n"
        )
    ));
    assert_eq!(
        two_ranges,
        rows(&[
            ("base", "03-02T08:00", "03-02T08:00"),
            ("move", "03-03T10:00", "03-03T08:00"),
            ("move", "03-04T10:00", "03-04T08:00"),
            ("move", "03-05T15:00", "03-05T08:00"),
            ("move", "03-06T15:00", "03-06T08:00"),
        ])
    );
    let single_on_a_moved_one = in_any_range(&format!(
        "{series}{}{}",
        override_of(
            ";RANGE=THISANDFUTURE:20260303T080000Z",
            "DTSTART:20260303T100000Z\nDURATION:PT45M\n"
        ),
        override_of(
            ":20260304T080000Z",
            "DTSTART:20260301T070000Z\nDURATION:PT15M\n"
        )
    ));
    assert_eq!(single_on_a_moved_one.len(), 5, "{single_on_a_moved_one:?}");
    assert_eq!(
        single_on_a_moved_one[0],
        rows(&[("move", "03-01T07:00", "03-04T08:00")])[0],
        "moved before the series starts"
    );
    // The first occurrence moved forward, out of February into March.
    let forward = in_any_range(&format!(
        "BEGIN:VEVENT
UID:s
SUMMARY:base
DTSTART:20260227T080000Z
DTEND:20260227T083000Z
RRULE:FREQ=DAILY;COUNT=3
END:VEVENT
{}",
        override_of(
            ";RANGE=THISANDFUTURE:20260227T080000Z",
            "DTSTART:20260302T080000Z\nDURATION:PT30M\n"
        )
    ));
    assert_eq!(
        forward
            .iter()
            .map(|(_, start, _)| start.as_str())
            .collect::<Vec<_>>(),
        ["03-02T08:00", "03-03T08:00", "03-04T08:00"]
    );
}

/// A series in Berlin across the change to summer time, with a range change and single
/// changes on both sides of it.
#[test]
fn a_zoned_series_over_a_clock_change_keeps_its_ids_in_any_range() {
    let found = in_any_range(
        "BEGIN:VEVENT
UID:s
SUMMARY:Berlin
DTSTART;TZID=Europe/Berlin:20260327T090000
DTEND;TZID=Europe/Berlin:20260327T093000
RRULE:FREQ=DAILY;COUNT=6
END:VEVENT
BEGIN:VEVENT
UID:s
SUMMARY:range
RECURRENCE-ID;RANGE=THISANDFUTURE;TZID=Europe/Berlin:20260328T090000
DTSTART;TZID=Europe/Berlin:20260328T110000
DURATION:PT30M
END:VEVENT
BEGIN:VEVENT
UID:s
SUMMARY:early single
RECURRENCE-ID;TZID=Europe/Berlin:20260327T090000
DTSTART;TZID=Europe/Berlin:20260330T130000
DURATION:PT20M
END:VEVENT
BEGIN:VEVENT
UID:s
SUMMARY:late single
RECURRENCE-ID;TZID=Europe/Berlin:20260330T090000
DTSTART;TZID=Europe/Berlin:20260401T120000
DURATION:PT35M
END:VEVENT
",
    );
    assert_eq!(found.len(), 6, "{found:?}");
    // 11:00 in Berlin on both sides of the change.
    assert!(found.contains(&(
        "range".to_string(),
        "03-28T10:00".to_string(),
        "s/2026-03-28T08:00:00+00:00".to_string()
    )));
    assert!(found.contains(&(
        "range".to_string(),
        "03-29T09:00".to_string(),
        "s/2026-03-29T07:00:00+00:00".to_string()
    )));
}

/// An override listing its own new start among its extra dates is still one meeting there;
/// its other extra date is a meeting of its own.
#[test]
fn an_override_repeating_its_start_in_its_extra_dates_is_one_meeting() {
    for rid in [":20260304T080000Z", ";RANGE=THISANDFUTURE:20260304T080000Z"] {
        let found = in_any_range(&format!(
            "{DAILY_FIVE}END:VEVENT\n{}",
            override_of(
                rid,
                "DTSTART:20260308T120000Z\nDURATION:PT45M\nRDATE:20260308T120000Z,20260312T100000Z\n"
            )
        ));
        let on = |day: &str| {
            found
                .iter()
                .filter(|(_, start, _)| start.starts_with(day))
                .count()
        };
        assert_eq!(on("03-08"), 1, "{rid}: {found:?}");
        assert_eq!(on("03-12"), 1, "{rid}: {found:?}");
    }
}

/// An explicit period keeps its own length: over the rule's default for the same start, and
/// through a "this and future" change that keeps the length; a change of length applies as
/// that change.
#[test]
fn an_explicit_period_keeps_its_length() {
    let night = |extra: &str, change: &str| {
        let text = feed(&format!(
            "BEGIN:VEVENT
UID:night
SUMMARY:night
DTSTART:20260302T220000Z
DURATION:PT30M
{extra}END:VEVENT
{change}"
        ));
        in_any_range(&text_events(&text));
        parse_events(
            &text,
            &CollectOptions {
                start: Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap(),
                end: Utc.with_ymd_and_hms(2026, 3, 9, 0, 0, 0).unwrap(),
            },
            Tz::UTC,
        )
        .unwrap()
        .into_iter()
        .map(|entry| {
            (
                entry.timestamp.as_str()[5..16].to_string(),
                entry.meeting.duration_minutes,
            )
        })
        .collect::<Vec<_>>()
    };
    let later = |minutes: u32| {
        format!(
            "BEGIN:VEVENT
UID:night
SUMMARY:later
RECURRENCE-ID;RANGE=THISANDFUTURE:20260303T220000Z
DTSTART:20260303T221500Z
DURATION:PT{minutes}M
END:VEVENT
"
        )
    };
    let period = "RDATE;VALUE=PERIOD:20260304T220000Z/PT12H\n";

    assert_eq!(
        night(&format!("RRULE:FREQ=DAILY;COUNT=3\n{period}"), ""),
        [
            ("03-02T22:00".to_string(), 30),
            ("03-03T22:00".to_string(), 30),
            ("03-04T22:00".to_string(), 720)
        ]
    );
    for extra in [
        format!("RRULE:FREQ=DAILY;COUNT=2\n{period}"),
        format!("RDATE:20260303T220000Z\n{period}"),
    ] {
        assert_eq!(
            night(&extra, &later(30)),
            [
                ("03-02T22:00".to_string(), 30),
                ("03-03T22:15".to_string(), 30),
                ("03-04T22:15".to_string(), 720)
            ],
            "{extra}"
        );
        // A change of length gives every later occurrence the new length (RFC 5545 §3.8.4.4).
        assert_eq!(
            night(&extra, &later(90)),
            [
                ("03-02T22:00".to_string(), 30),
                ("03-03T22:15".to_string(), 90),
                ("03-04T22:15".to_string(), 90)
            ],
            "{extra}"
        );
    }
}

/// The feed's events, for `in_any_range`.
fn text_events(text: &str) -> String {
    text.trim_start_matches("BEGIN:VCALENDAR\r\nVERSION:2.0\r\n")
        .trim_end_matches("END:VCALENDAR\r\n")
        .to_string()
}

/// A backward move in one series lengthens only that series' expansion: unrelated endless
/// series do not use up the budget of the others.
#[test]
fn a_backward_move_extends_only_its_own_series() {
    let mut events = String::new();
    for series in 0..4 {
        events.push_str(&format!(
            "BEGIN:VEVENT\nUID:future{series}\nDTSTART:20260309T080000Z\nDURATION:PT1M\nRRULE:FREQ=MINUTELY\nEND:VEVENT\n"
        ));
    }
    events.push_str(
        "BEGIN:VEVENT
UID:march
SUMMARY:Review meeting
DTSTART:20260302T080000Z
DURATION:PT30M
RRULE:FREQ=WEEKLY;COUNT=4
END:VEVENT
BEGIN:VEVENT
UID:summer
DTSTART:20260601T080000Z
DURATION:PT30M
RRULE:FREQ=WEEKLY;COUNT=3
END:VEVENT
BEGIN:VEVENT
UID:summer
SUMMARY:move
RECURRENCE-ID:20260608T080000Z
DTSTART:20260408T080000Z
DURATION:PT30M
END:VEVENT
",
    );
    let monday = CollectOptions {
        start: Utc.with_ymd_and_hms(2026, 3, 2, 0, 0, 0).unwrap(),
        end: Utc.with_ymd_and_hms(2026, 3, 3, 0, 0, 0).unwrap(),
    };

    let entries = parse_events(&feed(&events), &monday, Tz::UTC).unwrap();

    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.title.as_deref().unwrap())
            .collect::<Vec<_>>(),
        ["Review meeting"]
    );
}

/// Lengths from DTEND or DURATION on the series and its changes, all-day series with changes,
/// periods across midnight and the clock change, and changes carrying their own extra and
/// excluded dates.
#[test]
fn lengths_and_changes_with_dates_of_their_own() {
    let minutes = |events: &str| {
        let text = feed(events);
        in_any_range(events);
        parse_events(
            &text,
            &CollectOptions {
                start: Utc.with_ymd_and_hms(2026, 2, 1, 0, 0, 0).unwrap(),
                end: Utc.with_ymd_and_hms(2026, 5, 1, 0, 0, 0).unwrap(),
            },
            Tz::from_str("Europe/Berlin").unwrap(),
        )
        .unwrap()
        .into_iter()
        .map(|entry| {
            (
                entry.timestamp.as_str()[5..16].to_string(),
                entry.meeting.duration_minutes,
            )
        })
        .collect::<Vec<_>>()
    };
    let at = |start: &str, length: i32| (start.to_string(), length);

    // The series by DURATION, a single change by DTEND, a range change by DTEND.
    assert_eq!(
        minutes(
            "BEGIN:VEVENT
UID:s
DTSTART:20260302T080000Z
DURATION:PT30M
RRULE:FREQ=DAILY;COUNT=4
END:VEVENT
BEGIN:VEVENT
UID:s
RECURRENCE-ID:20260303T080000Z
DTSTART:20260303T090000Z
DTEND:20260303T091000Z
END:VEVENT
BEGIN:VEVENT
UID:s
RECURRENCE-ID;RANGE=THISANDFUTURE:20260304T080000Z
DTSTART:20260304T080000Z
DTEND:20260304T090000Z
END:VEVENT
"
        ),
        [
            at("03-02T08:00", 30),
            at("03-03T09:00", 10),
            at("03-04T08:00", 60),
            at("03-05T08:00", 60)
        ]
    );

    // All-day meetings are left out, changed or not, and take no timed one with them.
    assert_eq!(
        minutes(
            "BEGIN:VEVENT
UID:holidays
DTSTART;VALUE=DATE:20260302
DTEND;VALUE=DATE:20260303
RRULE:FREQ=DAILY;COUNT=3
END:VEVENT
BEGIN:VEVENT
UID:holidays
RECURRENCE-ID;VALUE=DATE:20260303
DTSTART;VALUE=DATE:20260310
DTEND;VALUE=DATE:20260311
END:VEVENT
BEGIN:VEVENT
UID:timed
DTSTART:20260303T080000Z
DURATION:PT30M
END:VEVENT
"
        ),
        [at("03-03T08:00", 30)]
    );

    // Overnight periods in Berlin, one across the change to summer time (23:00 to 05:00 is
    // five hours that night).
    assert_eq!(
        minutes(
            "BEGIN:VEVENT
UID:night
DTSTART;TZID=Europe/Berlin:20260327T230000
DURATION:PT30M
RDATE;TZID=Europe/Berlin;VALUE=PERIOD:20260328T230000/20260329T050000
END:VEVENT
"
        ),
        [at("03-27T22:00", 30), at("03-28T22:00", 300)]
    );

    // A range change whose extra date falls on one of the occurrences it moves, and that
    // carries an exclusion (which only the series' own exclusions do): its extra date is a
    // meeting of its own beside the moved one, and nothing is excluded.
    assert_eq!(
        minutes(
            "BEGIN:VEVENT
UID:s
DTSTART:20260302T080000Z
DURATION:PT30M
RRULE:FREQ=DAILY;COUNT=4
END:VEVENT
BEGIN:VEVENT
UID:s
RECURRENCE-ID;RANGE=THISANDFUTURE:20260303T080000Z
DTSTART:20260303T100000Z
DURATION:PT30M
RDATE:20260304T100000Z,20260310T100000Z
EXDATE:20260305T080000Z
END:VEVENT
"
        ),
        [
            at("03-02T08:00", 30),
            at("03-03T10:00", 30),
            at("03-04T10:00", 30),
            at("03-04T10:00", 30),
            at("03-05T10:00", 30),
            at("03-10T10:00", 30)
        ]
    );
}

/// An override's extra date is a meeting of its own, unless it repeats the override's own
/// start: then it is that meeting, and as a period it gives that meeting its end.
#[test]
fn an_overrides_extra_date_is_its_own_meeting_unless_it_repeats_its_start() {
    let pair = |range_extra: &str| {
        in_any_range(&format!(
            "BEGIN:VEVENT
UID:s
SUMMARY:base
DTSTART:20260302T080000Z
DTEND:20260302T081500Z
RRULE:FREQ=DAILY;COUNT=5
RDATE:20260307T080000Z
END:VEVENT
{}",
            override_of(
                ";RANGE=THISANDFUTURE:20260303T080000Z",
                &format!("DTSTART:20260303T100000Z\nDTEND:20260303T101500Z\n{range_extra}")
            )
        ))
    };
    let plain = pair("RDATE:20260307T100000Z\n");
    assert_eq!(plain.len(), 7, "{plain:?}");
    let period = pair("RDATE;VALUE=PERIOD:20260307T100000Z/PT16H\n");
    assert_eq!(period.len(), 7, "{period:?}");
    assert!(period.contains(&rows(&[("move", "03-07T10:00", "03-07T10:00")])[0]));

    let text = feed(&format!(
        "{DAILY_FIVE}END:VEVENT\n{}",
        override_of(
            ":20260304T080000Z",
            "DTSTART:20260308T120000Z\nDURATION:PT45M\nRDATE;VALUE=PERIOD:20260308T120000Z/PT15H\nRDATE:20260312T100000Z\n"
        )
    ));
    in_any_range(&text_events(&text));
    let ninth = parse_events(
        &text,
        &CollectOptions {
            start: Utc.with_ymd_and_hms(2026, 3, 9, 0, 0, 0).unwrap(),
            end: Utc.with_ymd_and_hms(2026, 3, 10, 0, 0, 0).unwrap(),
        },
        Tz::UTC,
    )
    .unwrap();
    assert_eq!(
        ninth
            .iter()
            .map(|entry| (entry.id.as_str(), entry.meeting.duration_minutes))
            .collect::<Vec<_>>(),
        [("s/2026-03-04T08:00:00+00:00", 900)]
    );
}

/// Shortening a two-hour meeting to 30 minutes from some date on gives a later 15-minute
/// period 30 minutes too, never less than nothing, on the day it then runs into.
#[test]
fn a_shortened_range_gives_later_meetings_its_length() {
    for (period, start) in [
        ("20260304T235000Z", "03-05T00:05"),
        ("20260304T233500Z", "03-04T23:50"),
    ] {
        let text = feed(&format!(
            "BEGIN:VEVENT
UID:night
SUMMARY:night
DTSTART:20260302T220000Z
DURATION:PT2H
RRULE:FREQ=DAILY;COUNT=2
RDATE;VALUE=PERIOD:{period}/PT15M
END:VEVENT
BEGIN:VEVENT
UID:night
SUMMARY:later
RECURRENCE-ID;RANGE=THISANDFUTURE:20260303T220000Z
DTSTART:20260303T221500Z
DURATION:PT30M
END:VEVENT
"
        ));
        in_any_range(&text_events(&text));
        let fifth = parse_events(
            &text,
            &CollectOptions {
                start: Utc.with_ymd_and_hms(2026, 3, 5, 0, 0, 0).unwrap(),
                end: Utc.with_ymd_and_hms(2026, 3, 6, 0, 0, 0).unwrap(),
            },
            Tz::UTC,
        )
        .unwrap();
        assert_eq!(
            fifth
                .iter()
                .map(|entry| (
                    entry.timestamp.as_str()[5..16].to_string(),
                    entry.meeting.duration_minutes
                ))
                .collect::<Vec<_>>(),
            [(start.to_string(), 30)],
            "{period}"
        );
    }
}

/// Each range change's length is compared with the anchor's length as earlier changes left
/// it, and an own-start period lengthens the anchor alone.
#[test]
fn range_lengths_chain_and_an_own_start_period_stays_with_its_anchor() {
    let lengths = |events: &str| {
        in_any_range(events);
        parse_events(
            &feed(events),
            &CollectOptions {
                start: Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap(),
                end: Utc.with_ymd_and_hms(2026, 3, 10, 0, 0, 0).unwrap(),
            },
            Tz::UTC,
        )
        .unwrap()
        .into_iter()
        .map(|entry| {
            (
                entry.timestamp.as_str()[5..16].to_string(),
                entry.meeting.duration_minutes,
            )
        })
        .collect::<Vec<_>>()
    };
    let at = |start: &str, minutes: i32| (start.to_string(), minutes);

    // Two hours, 30 minutes from March 3, two hours again from March 4.
    assert_eq!(
        lengths(
            "BEGIN:VEVENT
UID:reset
DTSTART:20260302T230000Z
DURATION:PT2H
RRULE:FREQ=DAILY;COUNT=4
END:VEVENT
BEGIN:VEVENT
UID:reset
RECURRENCE-ID;RANGE=THISANDFUTURE:20260303T230000Z
DTSTART:20260303T230000Z
DURATION:PT30M
END:VEVENT
BEGIN:VEVENT
UID:reset
RECURRENCE-ID;RANGE=THISANDFUTURE:20260304T230000Z
DTSTART:20260304T230000Z
DURATION:PT2H
END:VEVENT
"
        ),
        [
            at("03-02T23:00", 120),
            at("03-03T23:00", 30),
            at("03-04T23:00", 120),
            at("03-05T23:00", 120)
        ]
    );

    // Three hours from March 3, the March 3 meeting itself 30 minutes by its own period.
    assert_eq!(
        lengths(
            "BEGIN:VEVENT
UID:night
DTSTART:20260302T220000Z
DURATION:PT30M
RRULE:FREQ=DAILY;COUNT=4
END:VEVENT
BEGIN:VEVENT
UID:night
RECURRENCE-ID;RANGE=THISANDFUTURE:20260303T220000Z
DTSTART:20260303T221500Z
DURATION:PT3H
RDATE;VALUE=PERIOD:20260303T221500Z/PT30M
END:VEVENT
"
        ),
        [
            at("03-02T22:00", 30),
            at("03-03T22:15", 30),
            at("03-04T22:15", 180),
            at("03-05T22:15", 180)
        ]
    );
}

/// A meeting of 20 seconds across midnight is on both days.
#[test]
fn a_meeting_of_seconds_across_midnight_is_on_both_days() {
    let found = in_any_range(
        "BEGIN:VEVENT
UID:s
SUMMARY:midnight
DTSTART:20260302T235950Z
DTEND:20260303T000010Z
END:VEVENT
",
    );
    assert_eq!(found.len(), 1);
    let third = parse_events(
        &feed(
            "BEGIN:VEVENT\nUID:s\nDTSTART:20260302T235950Z\nDTEND:20260303T000010Z\nEND:VEVENT\n",
        ),
        &CollectOptions {
            start: Utc.with_ymd_and_hms(2026, 3, 3, 0, 0, 0).unwrap(),
            end: Utc.with_ymd_and_hms(2026, 3, 4, 0, 0, 0).unwrap(),
        },
        Tz::UTC,
    )
    .unwrap();
    assert_eq!(third.len(), 1);
}
