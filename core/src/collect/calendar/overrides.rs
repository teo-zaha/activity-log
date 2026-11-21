//! How a series' overrides (`RECURRENCE-ID`) change its planned occurrences.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use calcard::icalendar::{
    ICalendar, ICalendarComponent, ICalendarComponentType, ICalendarEntry, ICalendarParameterName,
    ICalendarProperty, timezone::TzResolver,
};
use chrono::{DateTime, Utc};

use super::{
    dates::instant_of,
    expand::{Occurrence, extra_dates, own_occurrence},
};

/// Applies the overrides among `members` to the planned occurrences. A "this and future"
/// override that names a planned occurrence moves it and every later one the series plans by
/// its own shift and gives them its properties, in the order of what they name; then each
/// single override replaces the occurrence it names. An override that names no planned
/// occurrence is a meeting of its own, named by its `RECURRENCE-ID` (none when cancelled),
/// and changes no other. An override's own extra dates are occurrences of their own, except
/// one at its own start, which is that meeting (a period gives it its end).
pub(super) fn apply_overrides(
    calendar: &ICalendar,
    members: &[usize],
    resolver: &TzResolver<String>,
    mut occurrences: Vec<Occurrence>,
) -> Vec<Occurrence> {
    let planned = occurrences
        .iter()
        .map(|occurrence| occurrence.planned)
        .collect::<BTreeSet<_>>();
    let mut ranges = Vec::new();
    let mut singles = BTreeMap::new();
    let mut unmatched = BTreeMap::new();
    let mut extras = Vec::new();
    for &index in members {
        let component = &calendar.components[index];
        let Some((named, this_and_future)) = recurrence_id(component, resolver) else {
            continue;
        };
        // A cancelled override often has no time of its own: it only takes its occurrence.
        let declared = own_occurrence(component, index, resolver).unwrap_or(Occurrence {
            component: index,
            planned: named,
            start: named,
            end: named,
            from_series: false,
        });
        // A period at its own start gives that meeting its end, and only that meeting.
        let mut own = declared;
        for (extra, explicit_end) in extra_dates(component, index, resolver) {
            if extra.start != own.start {
                extras.push(extra);
            } else if explicit_end {
                own.end = extra.end;
            }
        }
        let change = Occurrence {
            planned: named,
            ..own
        };
        if !planned.contains(&named) {
            unmatched.insert(named, change);
        } else if this_and_future {
            ranges.push((
                named,
                declared.end - declared.start,
                own.end - own.start,
                index,
            ));
        } else {
            // The later one in the feed wins.
            singles.insert(named, change);
        }
    }
    ranges.sort_by_key(|(named, ..)| *named);
    for &(from, declared, anchor_length, index) in &ranges {
        let component = &calendar.components[index];
        let start = own_occurrence(component, index, resolver).map_or(from, |own| own.start);
        let shift = start - from;
        // The declared length against the anchor's as earlier changes left it: a change gives
        // every later occurrence the declared length (RFC 5545 §3.8.4.4), else each keeps the
        // length it has.
        let current = occurrences
            .iter()
            .find(|occurrence| occurrence.planned == from)
            .map(|anchor| anchor.end - anchor.start);
        let new_length = (current != Some(declared)).then_some(declared);
        for occurrence in &mut occurrences {
            if occurrence.planned >= from && occurrence.from_series {
                let length = occurrence.end - occurrence.start;
                occurrence.component = index;
                occurrence.start = occurrence.planned + shift;
                occurrence.end = occurrence.start
                    + if occurrence.planned == from {
                        // Its own start's period, if any, is the anchor's alone.
                        anchor_length
                    } else {
                        new_length.unwrap_or(length)
                    };
            }
        }
    }
    for occurrence in &mut occurrences {
        if let Some(single) = singles.remove(&occurrence.planned) {
            *occurrence = single;
        }
    }
    let mut taken = planned;
    for extra in unmatched.into_values().chain(extras) {
        if taken.insert(extra.planned) {
            occurrences.push(extra);
        }
    }
    occurrences
}

/// For each UID, how far its overrides move a meeting back: RECURRENCE-ID less the new start,
/// the largest of them, when it is positive.
pub(super) fn moves_back(
    calendar: &ICalendar,
    resolver: &TzResolver<String>,
) -> HashMap<String, chrono::TimeDelta> {
    let mut back = HashMap::<String, chrono::TimeDelta>::new();
    for component in &calendar.components {
        if component.component_type != ICalendarComponentType::VEvent {
            continue;
        }
        let (Some(uid), Some((named, _)), Some(start)) = (
            component.uid(),
            recurrence_id(component, resolver),
            component
                .property(&ICalendarProperty::Dtstart)
                .and_then(|entry| instant_of(entry, None, resolver)),
        ) else {
            continue;
        };
        if named > start {
            let longest = back.entry(uid.to_string()).or_default();
            *longest = (*longest).max(named - start);
        }
    }
    back
}

/// `RECURRENCE-ID` as the planned instant it names, read in its zone as calcard reads it (its
/// own `TZID`, else the start's), and whether it is a "this and future" override.
pub(super) fn recurrence_id(
    component: &ICalendarComponent,
    resolver: &TzResolver<String>,
) -> Option<(DateTime<Utc>, bool)> {
    let entry = component.property(&ICalendarProperty::RecurrenceId)?;
    let start_tzid = component
        .property(&ICalendarProperty::Dtstart)
        .and_then(ICalendarEntry::tz_id);
    let planned = instant_of(entry, start_tzid, resolver)?;
    let this_and_future = entry
        .params
        .iter()
        .any(|param| param.name == ICalendarParameterName::Range);
    Some((planned, this_and_future))
}
