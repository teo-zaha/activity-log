//! Calendar days for the CLI. The backend only takes instants, so the CLI, like the dashboard
//! in the browser, decides where a day begins: in the terminal's local timezone.

mod preset;

use activity_log_core::model::CollectOptions;
use anyhow::{Result, anyhow};
use chrono::{DateTime, Datelike, Days, Duration, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};

pub use preset::RangePreset;

#[cfg(test)]
mod tests;

/// A local day as `--since` and `--until` take it, `YYYY-MM-DD`.
pub fn parse_day(value: &str) -> Result<NaiveDate, chrono::ParseError> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
}

pub fn today() -> NaiveDate {
    Local::now().date_naive()
}

/// First and last day, inclusive, of `preset` relative to `today`. Weeks start on Monday, as in
/// the dashboard.
pub fn resolve_preset(preset: RangePreset, today: NaiveDate) -> (NaiveDate, NaiveDate) {
    let days_since_monday = Days::new(today.weekday().num_days_from_monday().into());
    let first_of_month = today.with_day(1).expect("day 1 exists");

    match preset {
        RangePreset::Today => (today, today),
        RangePreset::Yesterday => {
            let day = today - Days::new(1);
            (day, day)
        }
        RangePreset::Week => (today - days_since_monday, today),
        RangePreset::LastWeek => {
            let end = today - days_since_monday - Days::new(1);
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

/// The first instant of `day` in `tz`: its midnight, or where a zone skipped midnight (a DST
/// gap, a change of offset), the first second its clocks showed on or after it. A zone that
/// skipped the whole day starts it with the next one.
fn start_of_day<Tz: TimeZone>(tz: &Tz, day: NaiveDate) -> Result<DateTime<Utc>> {
    let midnight = day.and_hms_opt(0, 0, 0).expect("midnight exists");
    let first_at = |local: NaiveDateTime| tz.from_local_datetime(&local).earliest();
    if let Some(start) = first_at(midnight) {
        return Ok(start.with_timezone(&Utc));
    }
    // Gaps end on a whole minute in every zone but a few historical ones; the seconds before
    // the first valid minute find those.
    let minute = (1..=MAX_GAP_MINUTES)
        .map(|minutes| midnight + Duration::minutes(minutes))
        .find(|local| first_at(*local).is_some())
        .ok_or_else(|| anyhow!("{day} has no start in the local timezone"))?;
    (1..=60)
        .rev()
        .map(|seconds| minute - Duration::seconds(seconds - 1))
        .find_map(first_at)
        .map(|start| start.with_timezone(&Utc))
        .ok_or_else(|| anyhow!("{day} has no start in the local timezone"))
}

/// The longest a zone's clocks ever skipped: a whole day (Samoa, 2011-12-30), with room.
const MAX_GAP_MINUTES: i64 = 2 * 24 * 60;
