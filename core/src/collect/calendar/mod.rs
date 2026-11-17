//! Events from an ICS feed: fetched over HTTPS (or read from a file), parsed, with recurring
//! series expanded into their occurrences in the requested range.

use std::{collections::HashMap, path::PathBuf, str::FromStr};

use anyhow::{Context, Result, anyhow, bail};
use calcard::{
    common::{PartialDateTime, timezone::Tz},
    icalendar::{
        ICalendar, ICalendarComponent, ICalendarComponentType, ICalendarEntry,
        ICalendarParameterName, ICalendarParameterValue, ICalendarPeriod, ICalendarProperty,
        ICalendarStatus, ICalendarValue, dates::TimeOrDelta, utils::strip_mailto_scheme,
    },
};
use chrono::{DateTime, Datelike, Days, Timelike, Utc};
use reqwest::Url;

use crate::{
    diagnostics,
    model::{CalendarEntry, CollectOptions},
};

#[cfg(test)]
mod tests;

/// Stops a feed with runaway recurrences; far more than any range holds.
const MAX_OCCURRENCES: usize = 200_000;

/// Where a feed is read from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FeedLocation {
    Url(Url),
    File(PathBuf),
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
            let host = url.host_str().unwrap_or("the calendar host").to_string();
            let client = crate::provider::http_client_builder()
                .build()
                .map_err(|err| anyhow!("HTTP client setup failed: {}", err.without_url()))?;
            let response = client
                .get(url)
                .send()
                .await
                .map_err(|err| anyhow!("{host} unreachable: {}", err.without_url()))?;
            let status = response.status();
            if !status.is_success() {
                bail!("{host} answered {status}; the link may have been unpublished");
            }
            response.text().await.map_err(|err| {
                anyhow!(
                    "Reading the calendar from {host} failed: {}",
                    err.without_url()
                )
            })
        }
    }
}

pub async fn collect_calendar(raw: &str, options: &CollectOptions) -> Result<Vec<CalendarEntry>> {
    let text = read_feed(raw).await?;
    parse_events(&text, options, local_zone())
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
pub(crate) fn parse_events(
    text: &str,
    options: &CollectOptions,
    floating_zone: Tz,
) -> Result<Vec<CalendarEntry>> {
    let mut calendar = ICalendar::parse(text)
        .map_err(|_| anyhow!("The link does not return an iCalendar feed"))?;
    end_open_series(&mut calendar, options.end + Days::new(1));
    prepare_expansion(&mut calendar, floating_zone)?;
    let expanded = calendar.expand_dates(floating_zone, MAX_OCCURRENCES);
    for error in &expanded.errors {
        let uid = calendar.components[error.comp_id as usize]
            .uid()
            .unwrap_or("(missing UID)");
        diagnostics::warn(
            "calendar",
            format!("Could not expand event UID {uid}: {}", error.error),
        );
    }
    if expanded.events.len() >= MAX_OCCURRENCES {
        diagnostics::warn(
            "calendar",
            format!("Stopped after {MAX_OCCURRENCES} occurrences; some events may be missing"),
        );
    }

    let mut entries = expanded
        .events
        .into_iter()
        .filter_map(|event| {
            let component = calendar.components.get(event.comp_id as usize)?;
            if component.component_type != ICalendarComponentType::VEvent
                || component.status() == Some(&ICalendarStatus::Cancelled)
                || is_all_day(component)
            {
                return None;
            }
            let start = event.start.to_utc();
            let end = match event.end {
                TimeOrDelta::Time(end) => end.to_utc(),
                TimeOrDelta::Delta(duration) => start + duration,
            };
            options
                .overlaps(start, end)
                .then(|| entry(component, start, end))
        })
        .collect::<Vec<_>>();
    entries.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
    Ok(entries)
}

fn prepare_expansion(calendar: &mut ICalendar, floating_zone: Tz) -> Result<()> {
    let resolver = calendar
        .build_owned_tz_resolver()
        .with_default(floating_zone);
    let mut series = HashMap::new();
    for (index, component) in calendar.components.iter_mut().enumerate() {
        if component.component_type != ICalendarComponentType::VEvent {
            continue;
        }
        let start_tzid = component
            .property(&ICalendarProperty::Dtstart)
            .and_then(ICalendarEntry::tz_id)
            .map(str::to_string);
        let start_zone = component
            .property(&ICalendarProperty::Dtstart)
            .and_then(|entry| entry.values.first())
            .and_then(ICalendarValue::as_partial_date_time)
            .and_then(PartialDateTime::to_date_time)
            .and_then(|start| start.tz())
            .unwrap_or_else(|| resolver.resolve_or_default(start_tzid.as_deref()));

        for entry in &component.entries {
            if matches!(
                entry.name,
                ICalendarProperty::Dtstart
                    | ICalendarProperty::Dtend
                    | ICalendarProperty::RecurrenceId
                    | ICalendarProperty::Exdate
                    | ICalendarProperty::Rdate
            ) && entry.values.iter().any(|value| {
                let dates = match value {
                    ICalendarValue::PartialDateTime(date) => [Some(date.as_ref()), None],
                    ICalendarValue::Period(ICalendarPeriod::Range { start, end }) => {
                        [Some(start), Some(end)]
                    }
                    ICalendarValue::Period(ICalendarPeriod::Duration { start, .. }) => {
                        [Some(start), None]
                    }
                    _ => [None, None],
                };
                dates.into_iter().flatten().any(|date| {
                    date.has_time()
                        && date.tz_hour.is_none()
                        && resolver
                            .resolve_or_default(entry.tz_id().or(start_tzid.as_deref()))
                            .is_floating()
                })
            }) {
                bail!(
                    "Event UID {} has a floating or unknown time zone; use UTC or an explicit TZID",
                    component.uid().unwrap_or("(missing UID)")
                );
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
    Ok(())
}

/// Gives every series without an end one shortly after the range, so expanding a daily
/// meeting started years ago stops at the range instead of running far into the future.
fn end_open_series(calendar: &mut ICalendar, until: DateTime<Utc>) {
    let until = PartialDateTime {
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

fn entry(
    component: &ICalendarComponent,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> CalendarEntry {
    CalendarEntry {
        source: "calendar".to_string(),
        connection: String::new(),
        timestamp: start.to_rfc3339(),
        title: text(component, ICalendarProperty::Summary)
            .unwrap_or_else(|| "(No title)".to_string()),
        location: text(component, ICalendarProperty::Location),
        organizer: organizer(component),
        duration_minutes: i32::try_from((end - start).num_minutes()).unwrap_or(i32::MAX),
    }
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
