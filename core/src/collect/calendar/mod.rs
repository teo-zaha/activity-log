//! Events from an ICS feed: fetched over HTTPS (or read from a file), parsed, with recurring
//! series expanded into their occurrences in the requested range.

use std::{
    collections::HashMap,
    str::FromStr,
    sync::atomic::{AtomicBool, Ordering},
};

use anyhow::{Result, anyhow, bail};
use calcard::{common::timezone::Tz, icalendar::ICalendar};
use chrono::{DateTime, Days, Utc};

use crate::{
    model::{CalendarEntry, CollectOptions},
    platform::blocking::{Stopped, blocking_until_dropped},
};

mod dates;
mod entry;
mod expand;
mod feed;
mod overrides;
mod prepare;
#[cfg(test)]
mod tests;

pub(crate) use feed::FeedUnavailable;
#[cfg(test)]
pub(crate) use feed::feed_location;
pub use feed::{check_feed, read_feed};

use entry::Prepared;
use expand::expand;
use overrides::moves_back;
use prepare::{end_open_series, prepare_expansion};

pub async fn collect_calendar(raw: &str, options: &CollectOptions) -> Result<Vec<CalendarEntry>> {
    let text = read_feed(raw).await?;
    let options = *options;
    // A large feed takes a while to expand: the other connections go on meanwhile, and a
    // stopped collection stops it between series.
    blocking_until_dropped("Reading the calendar", move |cancelled| {
        parse_events_until(&text, &options, local_zone(), cancelled)
    })
    .await?
}

/// Times without a zone are wall-clock times of this computer, the one place the backend
/// reads its time zone (see AGENTS.md).
fn local_zone() -> Tz {
    iana_time_zone::get_timezone()
        .ok()
        .and_then(|name| Tz::from_str(&name).ok())
        .unwrap_or(Tz::UTC)
}

/// The events of a feed that overlap `options`, one per occurrence. All-day and cancelled
/// events are left out.
#[cfg(test)]
pub(crate) fn parse_events(
    text: &str,
    options: &CollectOptions,
    floating_zone: Tz,
) -> Result<Vec<CalendarEntry>> {
    parse_events_until(text, options, floating_zone, &AtomicBool::new(false))
}

/// How many events are read between two looks at the cancel flag.
const BATCH: usize = 1_000;

/// [`parse_events`], ending with [`Stopped`] once `cancelled` is set: it is looked at between
/// series and every [`BATCH`] events.
pub(crate) fn parse_events_until(
    text: &str,
    options: &CollectOptions,
    floating_zone: Tz,
    cancelled: &AtomicBool,
) -> Result<Vec<CalendarEntry>> {
    Ok(entries_with_ends(text, options, floating_zone, cancelled)?
        .into_iter()
        .map(|(entry, _)| entry)
        .collect())
}

/// [`parse_events_until`], each entry with the instant it ends (the entry has whole minutes).
pub(crate) fn entries_with_ends(
    text: &str,
    options: &CollectOptions,
    floating_zone: Tz,
    cancelled: &AtomicBool,
) -> Result<Vec<(CalendarEntry, DateTime<Utc>)>> {
    let mut calendar = ICalendar::parse(text)
        .map_err(|_| anyhow!("The link does not return an iCalendar feed"))?;
    let resolver = calendar
        .build_owned_tz_resolver()
        .with_default(floating_zone);
    // Far enough that an occurrence planned after the range and moved back into it is planned.
    let moved_back = moves_back(&calendar, &resolver);
    end_open_series(&mut calendar, options.end + Days::new(1), &moved_back);
    prepare_expansion(&mut calendar, floating_zone);
    let occurrences = expand(&mut calendar, floating_zone, &resolver, cancelled)?;

    // What every occurrence of a component shares is read once per component, and only for
    // components with an occurrence in the range.
    let mut prepared = HashMap::<usize, Option<Prepared>>::new();
    let mut entries = Vec::new();
    for (index, occurrence) in occurrences.into_iter().enumerate() {
        if index % BATCH == 0 && cancelled.load(Ordering::Relaxed) {
            bail!(Stopped);
        }
        if !options.overlaps(occurrence.start, occurrence.end) {
            continue;
        }
        let prepared = prepared
            .entry(occurrence.component)
            .or_insert_with(|| Prepared::of(&calendar.components[occurrence.component]));
        if let Some(prepared) = prepared {
            entries.push((
                prepared.entry(occurrence.planned, occurrence.start, occurrence.end),
                occurrence.end,
            ));
        }
    }
    entries.sort_by(|(a, _), (b, _)| a.timestamp.cmp(&b.timestamp));
    Ok(entries)
}
