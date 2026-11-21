use activity_log_core::model::RangeError;
use chrono::{FixedOffset, NaiveDate, TimeZone, Utc};

use super::{RangePreset, day_range, resolve_preset};

fn date(value: &str) -> NaiveDate {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
}

#[test]
fn resolves_presets_relative_to_today() {
    // A Wednesday.
    let today = date("2026-03-25");
    let cases = [
        (RangePreset::Today, "2026-03-25", "2026-03-25"),
        (RangePreset::Yesterday, "2026-03-24", "2026-03-24"),
        (RangePreset::Week, "2026-03-23", "2026-03-25"),
        (RangePreset::LastWeek, "2026-03-16", "2026-03-22"),
        (RangePreset::Month, "2026-03-01", "2026-03-25"),
        (RangePreset::LastMonth, "2026-02-01", "2026-02-28"),
    ];
    for (preset, since, until) in cases {
        assert_eq!(
            resolve_preset(preset, today),
            (date(since), date(until)),
            "{preset}"
        );
    }
    assert_eq!(
        resolve_preset(RangePreset::LastMonth, date("2026-01-10")),
        (date("2025-12-01"), date("2025-12-31"))
    );
    assert_eq!(
        resolve_preset(RangePreset::Week, date("2026-03-29")),
        (date("2026-03-23"), date("2026-03-29"))
    );
}

#[test]
fn day_range_starts_at_the_first_valid_second_after_a_midnight_gap() {
    let range = day_range(
        &chrono_tz::Asia::Kathmandu,
        date("1986-01-01"),
        date("1986-01-01"),
    )
    .unwrap();
    assert_eq!(
        range.start,
        Utc.with_ymd_and_hms(1985, 12, 31, 18, 30, 0).unwrap()
    );
    assert_eq!(
        range.end,
        Utc.with_ymd_and_hms(1986, 1, 1, 18, 15, 0).unwrap()
    );
}

/// Samoa skipped 2011-12-30: the day starts and ends where the next one starts, so it holds no
/// time to collect.
#[test]
fn day_range_rejects_a_day_the_zone_skipped() {
    let error = day_range(
        &chrono_tz::Pacific::Apia,
        date("2011-12-30"),
        date("2011-12-30"),
    )
    .unwrap_err();
    assert_eq!(error.downcast_ref::<RangeError>(), Some(&RangeError::Empty));
    let around = day_range(
        &chrono_tz::Pacific::Apia,
        date("2011-12-29"),
        date("2011-12-31"),
    )
    .unwrap();
    assert_eq!(
        around.end - around.start,
        chrono::Duration::hours(48),
        "the 29th and the 31st"
    );
}

#[test]
fn day_range_spans_whole_local_days() {
    let berlin_winter = FixedOffset::east_opt(3600).unwrap();
    let range = day_range(&berlin_winter, date("2026-03-25"), date("2026-03-26")).unwrap();

    assert_eq!(
        range.start,
        Utc.with_ymd_and_hms(2026, 3, 24, 23, 0, 0).unwrap()
    );
    assert_eq!(
        range.end,
        Utc.with_ymd_and_hms(2026, 3, 26, 23, 0, 0).unwrap()
    );
}

#[test]
fn day_range_rejects_reversed_dates() {
    assert!(day_range(&Utc, date("2026-03-26"), date("2026-03-25")).is_err());
}

#[test]
fn day_range_rejects_more_than_ten_years() {
    let error = day_range(&Utc, date("1000-01-01"), date("9999-12-31")).unwrap_err();
    assert_eq!(
        error.downcast_ref::<RangeError>(),
        Some(&RangeError::TooLong)
    );
    assert!(day_range(&Utc, date("2016-01-01"), date("2025-12-31")).is_ok());
}
