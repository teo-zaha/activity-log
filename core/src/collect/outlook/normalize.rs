//! Outlook's events as entries, for the Windows and macOS collectors.

use chrono::DateTime;

use crate::collect::meeting_text::meeting_text;
use crate::model::{
    CollectOptions, MeetingBody, MeetingWeight, OutlookEntry, OutlookTag, Timestamp,
    outlook::raw_outlook_event::RawOutlookEvent,
};

pub(super) fn normalize_outlook_events(
    events: Vec<RawOutlookEvent>,
    options: &CollectOptions,
) -> Vec<OutlookEntry> {
    events
        .into_iter()
        .filter(default_calendar_event)
        .filter_map(|event| normalize_outlook_event(event, options))
        .collect()
}

/// The entry for `event` if it overlaps `options`; `None` also when its start or end names no
/// instant.
fn normalize_outlook_event(
    event: RawOutlookEvent,
    options: &CollectOptions,
) -> Option<OutlookEntry> {
    let start = Timestamp::parse(event.start.as_str()).ok()?;
    let end = DateTime::parse_from_rfc3339(&event.end).ok()?;
    if !options.overlaps(start.utc(), end.to_utc()) {
        return None;
    }
    let duration_minutes = (end - start.instant()).num_minutes();
    let meeting = meeting_text(event.body.as_deref(), None, []);
    // A series' occurrences share Outlook's id, so the start tells them apart.
    let id = match &event.id {
        Some(id) => format!("{id}/{}", event.start),
        None => crate::utils::stable_id(&[
            &event.start,
            &event.end,
            event.subject.as_deref().unwrap_or_default(),
            event.organizer.as_deref().unwrap_or_default(),
        ]),
    };

    Some(OutlookEntry {
        source: OutlookTag,
        connection: crate::model::ConnectionName::unstamped(),
        id,
        timestamp: start,
        title: event.subject.filter(|subject| !subject.trim().is_empty()),
        location: event.location,
        organizer: event.organizer,
        is_organizer: event.is_organizer,
        meeting: MeetingBody {
            duration_minutes: i32::try_from(duration_minutes).unwrap_or(i32::MAX),
            busy_status: event.busy_status,
            meeting_weight: MeetingWeight::of(event.busy_status),
            description: meeting.description,
            description_blocks: meeting.blocks,
            join_url: meeting.join_url,
        },
    })
}

fn default_calendar_event(event: &RawOutlookEvent) -> bool {
    event.is_default_calendar.unwrap_or(true)
}
