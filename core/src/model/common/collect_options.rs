use chrono::{DateTime, NaiveDate, Utc};

/// The instants to collect, as the half-open range `[start, end)`.
///
/// The backend never decides where a day begins: callers (the dashboard in the browser, the
/// CLI in the terminal) turn their local calendar days into instants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CollectOptions {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

impl CollectOptions {
    /// The longest range collected: ten years. A longer one would page a source's whole history
    /// and write a cache row per day.
    pub const MAX_DAYS: i64 = 3_653;

    /// Why the range cannot be collected: empty, backwards or longer than `MAX_DAYS`.
    pub fn check(&self) -> Result<(), &'static str> {
        if self.end <= self.start {
            return Err("end must be after start");
        }
        if self.end - self.start > chrono::Duration::days(Self::MAX_DAYS) {
            return Err("A range can span at most 10 years (3653 days)");
        }
        Ok(())
    }

    pub fn contains(&self, instant: DateTime<Utc>) -> bool {
        self.start <= instant && instant < self.end
    }

    /// True when an entry running from `from` to `to` (a meeting) starts inside the range or is
    /// still running when it begins.
    pub fn overlaps(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> bool {
        self.contains(from) || (from < self.start && to > self.start)
    }

    /// UTC calendar days the range touches, first and last inclusive.
    pub fn utc_days(&self) -> (NaiveDate, NaiveDate) {
        let last = if self.end > self.start {
            self.end - chrono::Duration::nanoseconds(1)
        } else {
            self.start
        };
        (self.start.date_naive(), last.date_naive())
    }
}
