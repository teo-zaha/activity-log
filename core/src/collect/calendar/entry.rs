//! What an event's entry shows: its title, place, organizer, busy status, description and
//! join link.

use calcard::icalendar::{
    ICalendarComponent, ICalendarComponentType, ICalendarParameterName, ICalendarParameterValue,
    ICalendarProperty, ICalendarStatus, ICalendarTransparency, ICalendarValue,
    utils::strip_mailto_scheme,
};
use chrono::{DateTime, Utc};

use crate::{
    collect::meeting_text::{MeetingText, looks_like_html, meeting_link, meeting_text},
    model::{BusyStatus, CalendarEntry, CalendarTag, MeetingBody, MeetingWeight},
};

/// Properties some servers write for the online-meeting link, in order of preference.
const JOIN_URL_PROPERTIES: [&str; 3] = [
    "X-MICROSOFT-SKYPETEAMSMEETINGURL",
    "X-MICROSOFT-ONLINEMEETINGEXTERNALLINK",
    "X-GOOGLE-CONFERENCE",
];

fn is_all_day(component: &ICalendarComponent) -> bool {
    component
        .property(&ICalendarProperty::Dtstart)
        .and_then(|entry| entry.values.first())
        .and_then(ICalendarValue::as_partial_date_time)
        .is_some_and(|start| !start.has_time())
}

/// What all occurrences of one event share, read once.
pub(super) struct Prepared {
    uid: Option<String>,
    title: Option<String>,
    location: Option<String>,
    organizer: Option<String>,
    busy_status: Option<BusyStatus>,
    meeting: MeetingText,
}

impl Prepared {
    /// `None` for what is not shown: no VEVENT, cancelled or all day.
    pub(super) fn of(component: &ICalendarComponent) -> Option<Self> {
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
    pub(super) fn entry(
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
