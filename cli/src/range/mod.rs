//! Calendar days for the CLI. The backend only takes instants, so the CLI, like the dashboard
//! in the browser, decides where a day begins: in the terminal's local timezone.

mod preset;

use activity_log_core::model::CollectOptions;
use anyhow::{Result, anyhow};
use chrono::{DateTime, Datelike, Days, Local, NaiveDate, TimeZone, Utc};

pub use preset::RangePreset;

#[cfg(test)]
mod tests;

pub fn parse_date(value: &str, name: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| anyhow!("Invalid date for {name}: {value:?} (expected YYYY-MM-DD)"))
}

pub fn today() -> NaiveDate {
    Local::now().date_naive()
}

/// First and last day, inclusive, of `preset` relative to `today`.
pub fn resolve_preset(preset: RangePreset, today: NaiveDate) -> (NaiveDate, NaiveDate) {
    let days_since_sunday = Days::new(today.weekday().num_days_from_sunday().into());
    let first_of_month = today.with_day(1).expect("day 1 exists");

    match preset {
        RangePreset::Today => (today, today),
        RangePreset::Yesterday => {
            let day = today - Days::new(1);
            (day, day)
        }
        RangePreset::Week => (today - days_since_sunday, today),
        RangePreset::LastWeek => {
            let end = today - days_since_sunday - Days::new(1);
            (end - Days::new(6), end)
        }
        RangePreset::Month => (first_of_month, today),
        RangePreset::LastMonth => {
            let end = first_of_month - Days::new(1);
            (end.with_day(1).expect("day 1 exists"), end)
        }
    }
}

/// The instants from the start of `since` to the start of the day after `until`, in `tz`.
pub fn day_range<Tz: TimeZone>(
    tz: &Tz,
    since: NaiveDate,
    until: NaiveDate,
) -> Result<CollectOptions> {
    if until < since {
        return Err(anyhow!("--until {until} is before --since {since}"));
    }
    let next = until
        .succ_opt()
        .ok_or_else(|| anyhow!("--until {until} is out of range"))?;
    let options = CollectOptions {
        start: start_of_day(tz, since)?,
        end: start_of_day(tz, next)?,
    };
    options.check()?;
    Ok(options)
}

/// A day can start inside a DST gap (some zones switch at midnight); take the first instant.
fn start_of_day<Tz: TimeZone>(tz: &Tz, day: NaiveDate) -> Result<DateTime<Utc>> {
    let midnight = day.and_hms_opt(0, 0, 0).expect("midnight exists");
    (0..=2)
        .find_map(|hour| {
            tz.from_local_datetime(&(midnight + chrono::Duration::hours(hour)))
                .earliest()
        })
        .map(|start| start.with_timezone(&Utc))
        .ok_or_else(|| anyhow!("{day} has no start in the local timezone"))
}
