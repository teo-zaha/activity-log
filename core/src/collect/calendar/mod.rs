//! Events from an ICS feed: fetched over HTTPS (or read from a file), parsed, with recurring
//! series expanded into their occurrences in the requested range.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::PathBuf,
    str::FromStr,
    sync::atomic::{AtomicBool, Ordering},
};

use anyhow::{Context, Result, anyhow, bail};
use calcard::{
    common::{PartialDateTime, timezone::Tz},
    icalendar::{
        ICalendar, ICalendarComponent, ICalendarComponentType, ICalendarEntry, ICalendarParameter,
        ICalendarParameterName, ICalendarParameterValue, ICalendarPeriod, ICalendarProperty,
        ICalendarStatus, ICalendarTransparency, ICalendarValue,
        dates::{CalendarEvent, TimeOrDelta},
        timezone::TzResolver,
        utils::strip_mailto_scheme,
    },
};
use chrono::{DateTime, Datelike, Days, Timelike, Utc};
use reqwest::Url;

use super::meeting_text::{MeetingText, looks_like_html, meeting_link, meeting_text};
use crate::{
    diagnostics,
    model::{BusyStatus, CalendarEntry, CalendarTag, CollectOptions, MeetingBody, MeetingWeight},
    platform::blocking::{Stopped, blocking_until_dropped},
    provider::remote_error::{ApiError, RemoteFailure},
};

#[cfg(test)]
mod tests;

/// How many occurrences one recurring series may run through, those before the range and
/// those it skips included; a series running longer is cut there with a warning, and the
/// others keep their share.
const MAX_SERIES_OCCURRENCES: usize = 50_000;
/// How many occurrences all series of a feed may run through together.
const MAX_OCCURRENCES: usize = 200_000;

/// Properties some servers write for the online-meeting link, in order of preference.
const JOIN_URL_PROPERTIES: [&str; 3] = [
    "X-MICROSOFT-SKYPETEAMSMEETINGURL",
    "X-MICROSOFT-ONLINEMEETINGEXTERNALLINK",
    "X-GOOGLE-CONFERENCE",
];

/// Where a feed is read from.
#[derive(Clone, PartialEq, Eq)]
pub(crate) enum FeedLocation {
    Url(Url),
    File(PathBuf),
}

impl std::fmt::Debug for FeedLocation {
    // A feed's link grants access to the calendar: only its host.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Url(url) => write!(f, "Url({})", url.host_str().unwrap_or_default()),
            Self::File(path) => f.debug_tuple("File").field(path).finish(),
        }
    }
}

/// `https://…`, `webcal://…` (read over HTTPS), an absolute path or a `file://` URL. Plain
/// `http://` is refused: the link works like a password.
pub(crate) fn feed_location(raw: &str) -> Result<FeedLocation> {
    let raw = raw.trim();
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        return Ok(FeedLocation::File(path));
    }
    let raw = match raw.get(..9) {
        Some(scheme) if scheme.eq_ignore_ascii_case("webcal://") => {
            format!("https://{}", &raw[9..])
        }
        _ => raw.to_string(),
    };
    let url = Url::parse(&raw).map_err(|_| {
        anyhow!("Expected an https:// or webcal:// link, or the path of an .ics file")
    })?;
    match url.scheme() {
        "https" => Ok(FeedLocation::Url(url)),
        "file" => url
            .to_file_path()
            .map(FeedLocation::File)
            .map_err(|()| anyhow!("Not a file path")),
        "http" => bail!("Use an https:// link; calendar links are secrets"),
        other => bail!("Unsupported link type {other}://"),
    }
}

/// The feed's text. Errors never contain the link, which grants access to the calendar.
pub async fn read_feed(raw: &str) -> Result<String> {
    match feed_location(raw)? {
        // A local export is small; reading it blocks only briefly.
        FeedLocation::File(path) => {
            std::fs::read_to_string(&path).with_context(|| format!("Reading {}", path.display()))
        }
        FeedLocation::Url(url) => {
            let (host, response) = request_feed(url).await?;
            response.text().await.map_err(|err| {
                anyhow::Error::new(RemoteFailure {
                    service: host.into(),
                    error: ApiError::from_transport(&err),
                })
            })
        }
    }
}

/// Whether the feed can be read, without reading it: the collection that follows downloads it,
/// and a feed is often large. A link answers with a success status; a file can be opened.
pub async fn check_feed(raw: &str) -> Result<()> {
    match feed_location(raw)? {
        FeedLocation::File(path) => {
            let metadata = std::fs::File::open(&path)
                .and_then(|file| file.metadata())
                .with_context(|| format!("Reading {}", path.display()))?;
            if !metadata.is_file() {
                bail!("{} is no file", path.display());
            }
            Ok(())
        }
        FeedLocation::Url(url) => request_feed(url).await.map(drop),
    }
}

/// The answer to a feed's link once its status is known, before its body is read. A failure
/// keeps how it failed (`RemoteFailure`: unreachable, timed out, refused or another status),
/// named by the host, never by the link.
async fn request_feed(url: Url) -> Result<(String, reqwest::Response)> {
    let host = url.host_str().unwrap_or("the calendar host").to_string();
    let failed = |error| RemoteFailure {
        service: host.clone().into(),
        error,
    };
    let client = crate::provider::http_client_builder()
        .build()
        .map_err(|err| anyhow!("HTTP client setup failed: {}", err.without_url()))?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|err| failed(ApiError::from_transport(&err)))?;
    let status = response.status();
    if !status.is_success() {
        return Err(failed(ApiError::Status {
            status,
            message: String::new(),
            retry_after: None,
        })
        .into());
    }
    Ok((host, response))
}

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

/// One occurrence of an event: when it was planned, which names it, and when it takes place
/// with the properties of `component` (the series' or an override's).
#[derive(Debug, Clone, Copy)]
struct Occurrence {
    component: usize,
    planned: DateTime<Utc>,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    /// Planned by the series itself, by its rule or its extra dates: a "this and future"
    /// override moves it.
    from_series: bool,
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
fn expand(
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
fn extra_dates(
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

/// The times a date list property (`RDATE`, `EXDATE`) holds, each with its end for a period;
/// in its zone, else the start's.
fn dates_of<'a>(
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
fn duration_of(component: &ICalendarComponent, resolver: &TzResolver<String>) -> chrono::TimeDelta {
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

/// Applies the overrides among `members` to the planned occurrences. A "this and future"
/// override that names a planned occurrence moves it and every later one the series plans by
/// its own shift and gives them its properties, in the order of what they name; then each
/// single override replaces the occurrence it names. An override that names no planned
/// occurrence is a meeting of its own, named by its `RECURRENCE-ID` (none when cancelled),
/// and changes no other. An override's own extra dates are occurrences of their own, except
/// one at its own start, which is that meeting (a period gives it its end).
fn apply_overrides(
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
fn moves_back(
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

/// A component's own occurrence: its `DTSTART` and its end.
fn own_occurrence(
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

/// `RECURRENCE-ID` as the planned instant it names, read in its zone as calcard reads it (its
/// own `TZID`, else the start's), and whether it is a "this and future" override.
fn recurrence_id(
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

/// The key `prepare_expansion` stores as `SEQUENCE`: one per UID.
fn series_key(component: &ICalendarComponent) -> i64 {
    component
        .property(&ICalendarProperty::Sequence)
        .and_then(|entry| entry.values.first())
        .and_then(|value| match value {
            ICalendarValue::Integer(key) => Some(*key),
            _ => None,
        })
        .unwrap_or_default()
}

/// The first value of a date property as an instant: in its own zone, else `fallback_tzid`'s.
fn instant_of(
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

/// Readies the feed for calcard. A time whose `TZID` names a zone neither the feed nor the IANA
/// database knows is read as this computer's time, like a time without a zone, and said so.
fn prepare_expansion(calendar: &mut ICalendar, floating_zone: Tz) {
    let resolver = calendar
        .build_owned_tz_resolver()
        .with_default(floating_zone);
    let mut series = HashMap::new();
    let mut unknown_zones = BTreeMap::<String, String>::new();
    for (index, component) in calendar.components.iter_mut().enumerate() {
        if component.component_type != ICalendarComponentType::VEvent {
            continue;
        }
        let start_tzid = component
            .property(&ICalendarProperty::Dtstart)
            .and_then(ICalendarEntry::tz_id)
            .map(str::to_string);
        let start_offset_zone = component
            .property(&ICalendarProperty::Dtstart)
            .and_then(|entry| entry.values.first())
            .and_then(ICalendarValue::as_partial_date_time)
            .and_then(PartialDateTime::to_date_time)
            .and_then(|start| start.tz());
        let start_zone =
            start_offset_zone.unwrap_or_else(|| resolver.resolve_or_default(start_tzid.as_deref()));
        // calcard steps through a series in the zone the start's `TZID` names, else in the
        // computer's: a start in UTC (`…Z`) gets that zone named, or a UTC series read in
        // Berlin would move by the offset and miss its overrides.
        if start_tzid.is_none()
            && let Some(name) = start_offset_zone.and_then(|zone| whole_hour_zone_name(&zone))
            && let Some(dtstart) = component
                .entries
                .iter_mut()
                .find(|entry| entry.name == ICalendarProperty::Dtstart)
        {
            dtstart.params.push(ICalendarParameter::tzid(name));
        }

        for entry in &component.entries {
            if matches!(
                entry.name,
                ICalendarProperty::Dtstart
                    | ICalendarProperty::Dtend
                    | ICalendarProperty::RecurrenceId
                    | ICalendarProperty::Exdate
                    | ICalendarProperty::Rdate
            ) && let Some(zone) = entry.tz_id()
                && resolver.resolve(zone).is_none()
            {
                unknown_zones
                    .entry(zone.to_string())
                    .or_insert_with(|| component.uid().unwrap_or("(missing UID)").to_string());
            }
        }

        // calcard 0.3 matches overrides by SEQUENCE, not UID. Give each UID its own key.
        let sequence = component.uid().map_or(index as i64, |uid| {
            *series.entry(uid.to_string()).or_insert(index as i64)
        });
        component
            .entries
            .retain(|entry| entry.name != ICalendarProperty::Sequence);
        component.entries.push(
            ICalendarEntry::new(ICalendarProperty::Sequence)
                .with_value(ICalendarValue::Integer(sequence)),
        );

        // calcard compares recurrence candidates in wall-clock time, including UTC UNTIL.
        for entry in &mut component.entries {
            if entry.name == ICalendarProperty::Rrule {
                for value in &mut entry.values {
                    if let ICalendarValue::RecurrenceRule(rule) = value
                        && let Some(until) = rule.until.as_ref()
                        && until.tz_hour.is_some()
                        && let Some(until) = until.to_date_time_with_tz(start_zone)
                    {
                        rule.until = Some(PartialDateTime::from_naive_timestamp(
                            until
                                .with_timezone(&start_zone)
                                .naive_local()
                                .and_utc()
                                .timestamp(),
                        ));
                    }
                }
            }
        }
    }
    for (zone, uid) in unknown_zones {
        diagnostics::warn(
            "calendar",
            format!(
                "Time zone {zone:?} (event UID {uid}) is unknown; its times are read as this computer's time"
            ),
        );
    }
}

/// `UTC`, or `Etc/GMT-1` for a whole-hour offset; `None` for another offset, which no IANA zone
/// names.
fn whole_hour_zone_name(zone: &Tz) -> Option<String> {
    match zone {
        Tz::Fixed(offset) if offset.local_minus_utc() % 3600 != 0 => None,
        _ if zone.is_utc() => Some("UTC".to_string()),
        _ => zone.name().map(|name| name.into_owned()),
    }
}

/// Gives every series without an end one shortly after the range, so expanding a daily
/// meeting started years ago stops at the range instead of running far into the future: at
/// `until`, later by as far as its own overrides move a meeting back (`moved_back`), so that
/// an occurrence planned after the range and moved into it is planned.
fn end_open_series(
    calendar: &mut ICalendar,
    until: DateTime<Utc>,
    moved_back: &HashMap<String, chrono::TimeDelta>,
) {
    let at = |until: DateTime<Utc>| PartialDateTime {
        year: u16::try_from(until.year()).ok(),
        month: Some(until.month() as u8),
        day: Some(until.day() as u8),
        hour: Some(until.hour() as u8),
        minute: Some(until.minute() as u8),
        second: Some(until.second() as u8),
        tz_hour: Some(0),
        tz_minute: Some(0),
        tz_minus: false,
    };
    for component in &mut calendar.components {
        let back = component
            .uid()
            .and_then(|uid| moved_back.get(uid))
            .copied()
            .unwrap_or_default();
        let until = at(until + back);
        for entry in &mut component.entries {
            if entry.name != ICalendarProperty::Rrule {
                continue;
            }
            for value in &mut entry.values {
                if let ICalendarValue::RecurrenceRule(rule) = value
                    && rule.until.is_none()
                    && rule.count.is_none()
                {
                    rule.until = Some(until.clone());
                }
            }
        }
    }
}

fn is_all_day(component: &ICalendarComponent) -> bool {
    component
        .property(&ICalendarProperty::Dtstart)
        .and_then(|entry| entry.values.first())
        .and_then(ICalendarValue::as_partial_date_time)
        .is_some_and(|start| !start.has_time())
}

/// What all occurrences of one event share, read once.
struct Prepared {
    uid: Option<String>,
    title: Option<String>,
    location: Option<String>,
    organizer: Option<String>,
    busy_status: Option<BusyStatus>,
    meeting: MeetingText,
}

impl Prepared {
    /// `None` for what is not shown: no VEVENT, cancelled or all day.
    fn of(component: &ICalendarComponent) -> Option<Self> {
        if component.component_type != ICalendarComponentType::VEvent
            || component.status() == Some(&ICalendarStatus::Cancelled)
            || is_all_day(component)
        {
            return None;
        }
        // Outlook adds the description as HTML (`X-ALT-DESC;FMTTYPE=text/html`) besides the text.
        let html = html_description(component);
        Some(Self {
            uid: component.uid().map(str::to_string),
            title: text(component, ICalendarProperty::Summary),
            location: text(component, ICalendarProperty::Location),
            organizer: organizer(component),
            busy_status: busy_status(component),
            meeting: meeting_text(
                text(component, ICalendarProperty::Description).as_deref(),
                html.as_deref(),
                explicit_join_urls(component).iter().map(String::as_str),
            ),
        })
    }

    /// The entry of an occurrence planned at `planned`, which names it.
    fn entry(
        &self,
        planned: DateTime<Utc>,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> CalendarEntry {
        let id = match &self.uid {
            // An occurrence is named by when it was planned, so a saved link finds it before and
            // after it moves.
            Some(uid) => format!("{uid}/{}", planned.to_rfc3339()),
            None => crate::utils::stable_id(&[
                self.title.as_deref().unwrap_or_default(),
                &start.to_rfc3339(),
                &end.to_rfc3339(),
            ]),
        };
        CalendarEntry {
            source: CalendarTag,
            connection: crate::model::ConnectionName::unstamped(),
            id,
            timestamp: start.into(),
            title: self.title.clone(),
            location: self.location.clone(),
            organizer: self.organizer.clone(),
            meeting: MeetingBody {
                duration_minutes: i32::try_from((end - start).num_minutes()).unwrap_or(i32::MAX),
                busy_status: self.busy_status,
                meeting_weight: MeetingWeight::of(self.busy_status),
                description: self.meeting.description.clone(),
                description_blocks: self.meeting.blocks.clone(),
                join_url: self.meeting.join_url.clone(),
            },
        }
    }
}

/// `X-ALT-DESC`, the description as HTML: when it says `FMTTYPE=text/html`, or else when its
/// text looks like HTML.
fn html_description(component: &ICalendarComponent) -> Option<String> {
    let declared = component
        .entries
        .iter()
        .find(|entry| {
            matches!(&entry.name, ICalendarProperty::Other(name) if name.eq_ignore_ascii_case("X-ALT-DESC"))
        })?
        .params
        .iter()
        .find(|param| param.name == ICalendarParameterName::Fmttype)
        .and_then(|param| param.value.as_text())
        .map(|format| format.trim().eq_ignore_ascii_case("text/html"));
    let html = other_property(component, "X-ALT-DESC")?;
    declared
        .unwrap_or_else(|| looks_like_html(&html))
        .then_some(html)
}

/// The values of `JOIN_URL_PROPERTIES`, then `URL` when it links to an online meeting (it can
/// also be any page about the event).
fn explicit_join_urls(component: &ICalendarComponent) -> Vec<String> {
    let mut urls = JOIN_URL_PROPERTIES
        .iter()
        .filter_map(|name| other_property(component, name))
        .collect::<Vec<_>>();
    urls.extend(text(component, ICalendarProperty::Url).and_then(|url| meeting_link(&url)));
    urls
}

/// An `X-` property as written: the parser splits unknown properties at `;`.
fn other_property(component: &ICalendarComponent, name: &str) -> Option<String> {
    let entry = component.entries.iter().find(|entry| {
        matches!(&entry.name, ICalendarProperty::Other(other) if other.eq_ignore_ascii_case(name))
    })?;
    let value = entry
        .values
        .iter()
        .filter_map(ICalendarValue::as_text)
        .collect::<Vec<_>>()
        .join(";");
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

/// Outlook's own free/busy value when the feed has it. Otherwise a transparent event blocks no
/// time (RFC 5545), so it is free even when tentative; else `STATUS:TENTATIVE` is tentative and
/// an opaque event busy. An occurrence moved or changed by an override is read from the override.
fn busy_status(component: &ICalendarComponent) -> Option<BusyStatus> {
    let microsoft = component
        .entries
        .iter()
        .find(|entry| {
            matches!(&entry.name, ICalendarProperty::Other(name)
                if name.eq_ignore_ascii_case("X-MICROSOFT-CDO-BUSYSTATUS"))
        })
        .and_then(|entry| entry.values.first())
        .and_then(ICalendarValue::as_text)
        .and_then(|value| match value.trim().to_ascii_uppercase().as_str() {
            "BUSY" => Some(BusyStatus::Busy),
            "TENTATIVE" => Some(BusyStatus::Tentative),
            "FREE" => Some(BusyStatus::Free),
            "OOF" => Some(BusyStatus::OutOfOffice),
            "WORKINGELSEWHERE" => Some(BusyStatus::WorkingElsewhere),
            _ => None,
        });
    let transparency = component.transparency();
    microsoft
        .or_else(|| {
            (transparency == Some(&ICalendarTransparency::Transparent)).then_some(BusyStatus::Free)
        })
        .or_else(|| {
            (component.status() == Some(&ICalendarStatus::Tentative))
                .then_some(BusyStatus::Tentative)
        })
        .or_else(|| {
            (transparency == Some(&ICalendarTransparency::Opaque)).then_some(BusyStatus::Busy)
        })
}

fn text(component: &ICalendarComponent, property: ICalendarProperty) -> Option<String> {
    component
        .property(&property)
        .and_then(|entry| entry.values.first())
        .and_then(ICalendarValue::as_text)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

/// The organizer's name, or their address without `mailto:`.
fn organizer(component: &ICalendarComponent) -> Option<String> {
    let entry = component.property(&ICalendarProperty::Organizer)?;
    entry
        .params
        .iter()
        .find(|param| param.name == ICalendarParameterName::Cn)
        .and_then(|param| match &param.value {
            ICalendarParameterValue::Text(name) => Some(name.trim().to_string()),
            _ => None,
        })
        .filter(|name| !name.is_empty())
        .or_else(|| {
            entry
                .values
                .first()
                .and_then(ICalendarValue::as_text)
                .map(|address| strip_mailto_scheme(address).to_string())
        })
}
