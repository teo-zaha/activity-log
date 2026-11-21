//! Date properties read as instants, each in its zone.

use calcard::{
    common::PartialDateTime,
    icalendar::{
        ICalendarComponent, ICalendarEntry, ICalendarPeriod, ICalendarProperty, ICalendarValue,
        timezone::TzResolver,
    },
};
use chrono::{DateTime, Utc};

/// The times a date list property (`RDATE`, `EXDATE`) holds, each with its end for a period;
/// in its zone, else the start's.
pub(super) fn dates_of<'a>(
    component: &'a ICalendarComponent,
    property: ICalendarProperty,
    resolver: &'a TzResolver<String>,
) -> impl Iterator<Item = (DateTime<Utc>, Option<DateTime<Utc>>)> + 'a {
    let start_tzid = component
        .property(&ICalendarProperty::Dtstart)
        .and_then(ICalendarEntry::tz_id);
    component
        .entries
        .iter()
        .filter(move |entry| entry.name == property)
        .flat_map(move |entry| {
            let zone = resolver.resolve_or_default(entry.tz_id().or(start_tzid));
            let at = move |date: &PartialDateTime| {
                date.has_time()
                    .then(|| date.to_date_time_with_tz(zone))
                    .flatten()
                    .map(|at| at.to_utc())
            };
            entry.values.iter().filter_map(move |value| match value {
                ICalendarValue::PartialDateTime(date) => Some((at(date)?, None)),
                ICalendarValue::Period(ICalendarPeriod::Range { start, end }) => {
                    Some((at(start)?, at(end)))
                }
                ICalendarValue::Period(ICalendarPeriod::Duration { start, duration }) => {
                    let start = at(start)?;
                    Some((
                        start,
                        duration.to_time_delta().map(|duration| start + duration),
                    ))
                }
                _ => None,
            })
        })
}

/// How long a component's event lasts: from `DTSTART` to `DTEND`, else its `DURATION`.
pub(super) fn duration_of(
    component: &ICalendarComponent,
    resolver: &TzResolver<String>,
) -> chrono::TimeDelta {
    let dtstart = component.property(&ICalendarProperty::Dtstart);
    let start = dtstart.and_then(|entry| instant_of(entry, None, resolver));
    let end = component
        .property(&ICalendarProperty::Dtend)
        .and_then(|entry| instant_of(entry, dtstart.and_then(ICalendarEntry::tz_id), resolver));
    match (start, end) {
        (Some(start), Some(end)) => end - start,
        _ => component
            .property(&ICalendarProperty::Duration)
            .and_then(|entry| entry.values.first())
            .and_then(|value| match value {
                ICalendarValue::Duration(duration) => duration.to_time_delta(),
                _ => None,
            })
            .unwrap_or_default(),
    }
}

/// The first value of a date property as an instant: in its own zone, else `fallback_tzid`'s.
pub(super) fn instant_of(
    entry: &ICalendarEntry,
    fallback_tzid: Option<&str>,
    resolver: &TzResolver<String>,
) -> Option<DateTime<Utc>> {
    let at = entry.values.first()?.as_partial_date_time()?;
    if !at.has_time() {
        return None;
    }
    let zone = resolver.resolve_or_default(entry.tz_id().or(fallback_tzid));
    Some(at.to_date_time_with_tz(zone)?.to_utc())
}
