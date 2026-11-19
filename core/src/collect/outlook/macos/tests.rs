use chrono::{TimeZone, Utc};

use super::{Fields, collect_script, script_bounds};
use crate::model::CollectOptions;

#[test]
fn the_range_goes_to_the_script_in_whole_seconds_covering_it() {
    let fractional = CollectOptions {
        start: Utc.with_ymd_and_hms(2026, 3, 2, 8, 0, 0).unwrap()
            + chrono::Duration::milliseconds(456),
        end: Utc.with_ymd_and_hms(2026, 3, 9, 7, 59, 59).unwrap()
            + chrono::Duration::microseconds(1),
    };
    assert_eq!(
        script_bounds(&fractional),
        (
            "2026-03-02T08:00:00Z".to_string(),
            "2026-03-09T08:00:00Z".to_string()
        )
    );

    let whole = CollectOptions {
        start: Utc.with_ymd_and_hms(2026, 3, 2, 0, 0, 0).unwrap(),
        end: Utc.with_ymd_and_hms(2026, 3, 9, 0, 0, 0).unwrap(),
    };
    assert_eq!(
        script_bounds(&whole),
        (
            "2026-03-02T00:00:00Z".to_string(),
            "2026-03-09T00:00:00Z".to_string()
        )
    );
}

/// The bounds as Foundation reads them in the script: it gives no date for a fraction.
#[tokio::test]
async fn foundation_reads_the_bounds_the_script_gets() {
    let dashboard_range = CollectOptions {
        start: chrono::DateTime::parse_from_rfc3339("2026-03-01T23:00:00.000Z")
            .unwrap()
            .to_utc(),
        end: chrono::DateTime::parse_from_rfc3339("2026-03-08T23:00:00.000Z")
            .unwrap()
            .to_utc(),
    };
    let rolling_range = CollectOptions {
        start: Utc::now() - chrono::Duration::days(7),
        end: Utc::now(),
    };
    let read = r#"use framework "Foundation"
on run argv
  set formatter to current application's NSISO8601DateFormatter's new()
  repeat with isoText in argv
    if (formatter's dateFromString:(contents of isoText)) is missing value then return "unread " & isoText
  end repeat
  return "read"
end run"#;
    for options in [dashboard_range, rolling_range] {
        let (start, end) = script_bounds(&options);
        let answer = crate::platform::macos::run_apple_script(read, &[&start, &end])
            .await
            .unwrap();
        assert_eq!(answer, "read", "{start} {end}");
    }
}

#[test]
fn only_the_full_script_names_busy_status_and_body() {
    let full = collect_script(Fields::All);
    let basic = collect_script(Fields::Basic);
    for term in ["free busy status", "plain text content"] {
        assert!(full.contains(term), "{term}");
        assert!(!basic.contains(term), "{term}");
    }
    assert!(!full.contains("--EXTRAS--"));
}

/// Device check for the busy status and body read: times the last 90 days and the last year
/// against the Outlook that is open, with and without them. On a Mac with Outlook running,
/// given how many meetings of the last 90 days at least have a description and a busy status:
/// `OUTLOOK_MIN_BODIES=5 OUTLOOK_MIN_STATUSES=5 cargo test -p core --lib outlook_timing -- --ignored --nocapture`
#[tokio::test]
#[ignore = "reads the running Outlook"]
async fn outlook_timing() {
    let at_least = |name: &str| {
        std::env::var(name)
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or_else(|| panic!("set {name}: how many the profile has at least"))
    };
    let (min_bodies, min_statuses) = (
        at_least("OUTLOOK_MIN_BODIES"),
        at_least("OUTLOOK_MIN_STATUSES"),
    );
    for days in [90, 365] {
        let options = CollectOptions {
            start: Utc::now() - chrono::Duration::days(days),
            end: Utc::now(),
        };
        let (start, end) = script_bounds(&options);
        for fields in [Fields::All, Fields::Basic] {
            let started = std::time::Instant::now();
            let output =
                crate::platform::macos::run_apple_script(&collect_script(fields), &[&start, &end])
                    .await
                    .unwrap();
            let elapsed = started.elapsed();
            let (events, unread) = super::super::parse_outlook_columns(&output).unwrap();
            let with =
                |field: fn(&crate::model::outlook::raw_outlook_event::RawOutlookEvent) -> bool| {
                    events.iter().filter(|event| field(event)).count()
                };
            println!(
                "{days} days, {fields:?}: {} events ({} with a body, {} with a busy status, {} with an organizer), {} KB, {elapsed:.2?}, unread: {unread:?}",
                events.len(),
                with(|event| event.body.is_some()),
                with(|event| event.busy_status.is_some()),
                with(|event| event.organizer.is_some()),
                output.len() / 1024,
            );
            assert!(
                elapsed < std::time::Duration::from_secs(120),
                "inside the collection limit"
            );
            if fields == Fields::All {
                assert!(
                    with(|event| event.body.is_some()) >= min_bodies,
                    "descriptions read"
                );
                assert!(
                    with(|event| event.busy_status.is_some()) >= min_statuses,
                    "busy statuses read"
                );
            }
        }
    }
}

/// The script's check that its lists still line up, run on lists as Outlook would give them:
/// two events at the same start trading places are caught by their subjects too, with ids or
/// without.
#[tokio::test]
async fn the_row_check_catches_a_swap_of_events_at_the_same_start() {
    let check = |ids: &str, swapped_ids: &str| {
        format!(
            r#"{handler}
on run
  set firstStart to current date
  set starts to {{firstStart, firstStart, firstStart + 3600}}
  set sameRows to my rowsLineUp({ids}, starts, {{"A", "B", "C"}}, {ids}, starts, {{"A", "B", "C"}})
  set swappedRows to my rowsLineUp({ids}, starts, {{"A", "B", "C"}}, {swapped_ids}, starts, {{"B", "A", "C"}})
  return (sameRows as text) & " " & (swappedRows as text)
end run"#,
            handler = super::ROWS_LINE_UP
        )
    };
    // Subjects that differ only in case are different subjects, as in the Rust key.
    let case_only = format!(
        r#"{handler}
on run
  set firstStart to current date
  set starts to {{firstStart, firstStart}}
  return (my rowsLineUp(missing value, starts, {{"Review", "REVIEW"}}, missing value, starts, {{"REVIEW", "Review"}})) as text
end run"#,
        handler = super::ROWS_LINE_UP
    );
    assert_eq!(
        crate::platform::macos::run_apple_script(&case_only, &[])
            .await
            .unwrap(),
        "false"
    );
    for (ids, swapped) in [
        ("missing value", "missing value"),
        (
            "{missing value, missing value, 27}",
            "{missing value, missing value, 27}",
        ),
        ("{13, 13, 27}", "{13, 13, 27}"),
        ("{13, 14, 27}", "{14, 13, 27}"),
    ] {
        let answer = crate::platform::macos::run_apple_script(&check(ids, swapped), &[])
            .await
            .unwrap();
        assert_eq!(answer, "true false", "{ids}");
    }
}
