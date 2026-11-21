//! Expanding the feed's events into their occurrences: recurring series by their planned
//! occurrences, on a budget per series and per feed.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::atomic::{AtomicBool, Ordering},
};

use anyhow::{Result, bail};
use calcard::{
    common::timezone::Tz,
    icalendar::{
        ICalendar, ICalendarComponent, ICalendarComponentType, ICalendarEntry, ICalendarProperty,
        dates::{CalendarEvent, TimeOrDelta},
        timezone::TzResolver,
    },
};
use chrono::{DateTime, Utc};

use super::{
    BATCH,
    dates::{dates_of, duration_of, instant_of},
    overrides::{apply_overrides, recurrence_id},
    prepare::series_key,
};
use crate::{diagnostics, platform::blocking::Stopped};

/// How many occurrences one recurring series may run through, those before the range and
/// those it skips included; a series running longer is cut there with a warning, and the
/// others keep their share.
const MAX_SERIES_OCCURRENCES: usize = 50_000;
/// How many occurrences all series of a feed may run through together.
const MAX_OCCURRENCES: usize = 200_000;

/// One occurrence of an event: when it was planned, which names it, and when it takes place
/// with the properties of `component` (the series' or an override's).
#[derive(Debug, Clone, Copy)]
pub(super) struct Occurrence {
    pub(super) component: usize,
    pub(super) planned: DateTime<Utc>,
    pub(super) start: DateTime<Utc>,
    pub(super) end: DateTime<Utc>,
    /// Planned by the series itself, by its rule or its extra dates: a "this and future"
    /// override moves it.
    pub(super) from_series: bool,
}

/// The occurrences of the feed's events.
///
/// A recurring series (a UID's components) is built from its planned occurrences: calcard
/// expands the series' rule alone, without its extra dates, exclusions and overrides, on a
/// budget of its own (calcard counts one budget across all series, so one long series could
/// use it up). Each date it runs through costs one, and the planned occurrences are those
/// dates, the extra dates (`RDATE`) less the excluded ones (`EXDATE`). The overrides are then
/// applied by their `RECURRENCE-ID`, which names a planned occurrence, never by calcard's
/// matching. Events that do not recur are expanded in batches of [`BATCH`].
pub(super) fn expand(
    calendar: &mut ICalendar,
    floating_zone: Tz,
    resolver: &TzResolver<String>,
    cancelled: &AtomicBool,
) -> Result<Vec<Occurrence>> {
    let zones = calendar
        .components
        .iter()
        .enumerate()
        .filter(|(_, component)| component.component_type == ICalendarComponentType::VTimezone)
        .filter_map(|(index, component)| {
            let tzid = component.property(&ICalendarProperty::Tzid)?;
            Some((tzid.values.first()?.as_text()?.to_string(), index))
        })
        .collect::<HashMap<_, _>>();
    let mut series = BTreeMap::<i64, Vec<usize>>::new();
    for (index, component) in calendar.components.iter().enumerate() {
        if component.component_type == ICalendarComponentType::VEvent {
            series.entry(series_key(component)).or_default().push(index);
        }
    }
    // A series: a component without RECURRENCE-ID (its master) that plans more than its
    // start, or that others change. Anything else (a lone event, changes whose series is not in
    // the feed) is expanded as it is.
    let is_series = |members: &Vec<usize>| {
        let component = |index: &usize| &calendar.components[*index];
        let has = |index: &usize, property: ICalendarProperty| {
            component(index).property(&property).is_some()
        };
        members
            .iter()
            .any(|index| !has(index, ICalendarProperty::RecurrenceId))
            && (members.len() > 1
                || members.iter().any(|index| {
                    has(index, ICalendarProperty::Rrule) || has(index, ICalendarProperty::Rdate)
                }))
    };
    let (recurring, singles): (Vec<_>, Vec<_>) = series.into_values().partition(is_series);

    let mut occurrences = Vec::new();
    for batch in singles.concat().chunks(BATCH) {
        if cancelled.load(Ordering::Relaxed) {
            bail!(Stopped);
        }
        for event in expand_group(calendar, &zones, batch, floating_zone, 0) {
            let component = event.comp_id as usize;
            let start = event.start.to_utc();
            let end = match event.end {
                TimeOrDelta::Time(end) => end.to_utc(),
                TimeOrDelta::Delta(duration) => start + duration,
            };
            // An override whose series is not in the feed is still named by what it replaces.
            let planned = recurrence_id(&calendar.components[component], resolver)
                .map_or(start, |(planned, _)| planned);
            occurrences.push(Occurrence {
                component,
                planned,
                start,
                end,
                from_series: false,
            });
        }
    }

    let mut remaining = MAX_OCCURRENCES;
    for members in &recurring {
        if cancelled.load(Ordering::Relaxed) {
            bail!(Stopped);
        }
        if remaining == 0 {
            diagnostics::warn(
                "calendar",
                format!(
                    "Stopped after {MAX_OCCURRENCES} occurrences in all; the remaining recurring events are missing"
                ),
            );
            break;
        }
        let share = remaining.min(MAX_SERIES_OCCURRENCES);
        let (planned, attempted, cut) =
            planned_occurrences(calendar, &zones, members, floating_zone, resolver, share);
        remaining -= attempted;
        if cut {
            let uid = calendar.components[members[0]]
                .uid()
                .unwrap_or("(missing UID)");
            diagnostics::warn(
                "calendar",
                format!(
                    "Stopped expanding event UID {uid} after {share} occurrences; some of its occurrences may be missing"
                ),
            );
        }
        occurrences.extend(apply_overrides(calendar, members, resolver, planned));
    }
    Ok(occurrences)
}

/// The planned occurrences of the series `members`: for each master (a member without
/// RECURRENCE-ID) its start and the dates of its rule, its extra dates (`RDATE`) less the
/// excluded ones (`EXDATE`), one occurrence per instant (RFC 5545 §3.8.5.2). Also how many
/// dates the rules ran through (at most `share`), and whether they were cut there.
fn planned_occurrences(
    calendar: &ICalendar,
    zones: &HashMap<String, usize>,
    members: &[usize],
    floating_zone: Tz,
    resolver: &TzResolver<String>,
    share: usize,
) -> (Vec<Occurrence>, usize, bool) {
    let mut planned = Vec::new();
    let mut instants = BTreeSet::new();
    let (mut attempted, mut cut) = (0, false);
    for &index in members {
        let component = &calendar.components[index];
        if component
            .property(&ICalendarProperty::RecurrenceId)
            .is_some()
        {
            continue;
        }
        let uid = component.uid().unwrap_or("(missing UID)");
        let mut own = Vec::new();
        if component.property(&ICalendarProperty::Rrule).is_some() && !cut {
            // The rule alone (its first date is the start): each date it runs through is one
            // occurrence. One more than what is left of the share tells a rule that ends
            // there from one cut off.
            let mut rule = component.clone();
            rule.entries.retain(|entry| {
                !matches!(
                    entry.name,
                    ICalendarProperty::Exdate
                        | ICalendarProperty::Rdate
                        | ICalendarProperty::Exrule
                )
            });
            let mut probe = ICalendar {
                components: zone_components(calendar, zones, &[index]),
            };
            probe.components.push(rule);
            let left = share - attempted;
            let expanded = probe.expand_dates(floating_zone, left + 1);
            for error in &expanded.errors {
                diagnostics::warn(
                    "calendar",
                    format!("Could not expand event UID {uid}: {}", error.error),
                );
            }
            cut |= expanded.events.len() > left;
            let dates = expanded.events.into_iter().take(left).collect::<Vec<_>>();
            attempted += dates.len();
            own.extend(dates.into_iter().map(|event| {
                let start = event.start.to_utc();
                let end = match event.end {
                    TimeOrDelta::Time(end) => end.to_utc(),
                    TimeOrDelta::Delta(duration) => start + duration,
                };
                Occurrence {
                    component: index,
                    planned: start,
                    start,
                    end,
                    from_series: true,
                }
            }));
        } else if component.property(&ICalendarProperty::Rrule).is_none() {
            match own_occurrence(component, index, resolver) {
                Some(start) => own.push(start),
                None => diagnostics::warn(
                    "calendar",
                    format!("Could not expand event UID {uid}: it has no start"),
                ),
            }
        }
        // An extra date given as a period keeps its own end over the rule's for that start.
        let (periods, dates): (Vec<_>, Vec<_>) =
            extra_dates(component, index, resolver).partition(|(_, explicit_end)| *explicit_end);
        own.splice(0..0, periods.into_iter().map(|(occurrence, _)| occurrence));
        own.extend(dates.into_iter().map(|(occurrence, _)| occurrence));
        let excluded = dates_of(component, ICalendarProperty::Exdate, resolver)
            .map(|(start, _)| start)
            .collect::<BTreeSet<_>>();
        for occurrence in own {
            if !excluded.contains(&occurrence.planned) && instants.insert(occurrence.planned) {
                planned.push(Occurrence {
                    from_series: true,
                    ..occurrence
                });
            }
        }
    }
    (planned, attempted, cut)
}

/// A component's `RDATE`s as occurrences of its own, each planned at its own time, and whether
/// it gives its own end (a period).
pub(super) fn extra_dates(
    component: &ICalendarComponent,
    index: usize,
    resolver: &TzResolver<String>,
) -> impl Iterator<Item = (Occurrence, bool)> {
    let duration = duration_of(component, resolver);
    dates_of(component, ICalendarProperty::Rdate, resolver)
        .map(move |(start, end)| {
            let occurrence = Occurrence {
                component: index,
                planned: start,
                start,
                end: end.unwrap_or(start + duration),
                from_series: false,
            };
            (occurrence, end.is_some())
        })
        .collect::<Vec<_>>()
        .into_iter()
}

/// A component's own occurrence: its `DTSTART` and its end.
pub(super) fn own_occurrence(
    component: &ICalendarComponent,
    index: usize,
    resolver: &TzResolver<String>,
) -> Option<Occurrence> {
    let start = instant_of(
        component.property(&ICalendarProperty::Dtstart)?,
        None,
        resolver,
    )?;
    Some(Occurrence {
        component: index,
        planned: start,
        start,
        end: start + duration_of(component, resolver),
        from_series: false,
    })
}

/// The VTIMEZONEs the components `members` name.
fn zone_components(
    calendar: &ICalendar,
    zones: &HashMap<String, usize>,
    members: &[usize],
) -> Vec<ICalendarComponent> {
    members
        .iter()
        .flat_map(|&index| &calendar.components[index].entries)
        .filter_map(ICalendarEntry::tz_id)
        .filter_map(|tzid| zones.get(tzid).copied())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|index| calendar.components[index].clone())
        .collect()
}

/// An occurrence as calcard expands it.
type CalcardEvent = CalendarEvent<DateTime<Tz>, TimeOrDelta<DateTime<Tz>, chrono::TimeDelta>>;

/// Expands the components `members` of `calendar` in a calendar of their own, with the time
/// zones they name; `comp_id`s are those of `calendar`.
fn expand_group(
    calendar: &mut ICalendar,
    zones: &HashMap<String, usize>,
    members: &[usize],
    floating_zone: Tz,
    limit: usize,
) -> Vec<CalcardEvent> {
    let mut group = ICalendar {
        components: zone_components(calendar, zones, members),
    };
    let first = group.components.len();
    group.components.extend(
        members
            .iter()
            .map(|&index| std::mem::take(&mut calendar.components[index])),
    );
    let expanded = group.expand_dates(floating_zone, limit);
    for error in &expanded.errors {
        let uid = group.components[error.comp_id as usize]
            .uid()
            .unwrap_or("(missing UID)");
        diagnostics::warn(
            "calendar",
            format!("Could not expand event UID {uid}: {}", error.error),
        );
    }
    for (component, &index) in group.components.drain(first..).zip(members) {
        calendar.components[index] = component;
    }
    expanded
        .events
        .into_iter()
        .filter_map(|mut event| {
            event.comp_id = *members.get((event.comp_id as usize).checked_sub(first)?)? as u32;
            Some(event)
        })
        .collect()
}
