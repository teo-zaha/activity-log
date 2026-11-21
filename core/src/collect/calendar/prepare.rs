//! Readying a feed for calcard: time zones it cannot resolve or would step through wrongly,
//! a series key per UID, and an end for every series without one.

use std::collections::{BTreeMap, HashMap};

use calcard::{
    common::{PartialDateTime, timezone::Tz},
    icalendar::{
        ICalendar, ICalendarComponent, ICalendarComponentType, ICalendarEntry, ICalendarParameter,
        ICalendarProperty, ICalendarValue,
    },
};
use chrono::{DateTime, Datelike, Timelike, Utc};

use crate::diagnostics;

/// The key `prepare_expansion` stores as `SEQUENCE`: one per UID.
pub(super) fn series_key(component: &ICalendarComponent) -> i64 {
    component
        .property(&ICalendarProperty::Sequence)
        .and_then(|entry| entry.values.first())
        .and_then(|value| match value {
            ICalendarValue::Integer(key) => Some(*key),
            _ => None,
        })
        .unwrap_or_default()
}

/// Readies the feed for calcard. A time whose `TZID` names a zone neither the feed nor the IANA
/// database knows is read as this computer's time, like a time without a zone, and said so.
pub(super) fn prepare_expansion(calendar: &mut ICalendar, floating_zone: Tz) {
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
pub(super) fn end_open_series(
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
