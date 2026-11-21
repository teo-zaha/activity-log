use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;
use chrono::{DateTime, Days, Utc};

use crate::{
    model::{CollectOptions, OutlookEntry, outlook::raw_outlook_event::RawOutlookEvent},
    platform::{
        ComGuard, DispatchObject, active_outlook_application,
        blocking::{Exclusive, Stopped, blocking_until_dropped},
        connect_outlook_application, process_is_elevated, running_process_names,
    },
};

use crate::provider::source_failure::SourceFailure;

use super::{
    OutlookFallback, busy_status_from_com, is_organizer_from_com,
    normalize::normalize_outlook_events,
};

/// Guards against a collection that never ends, such as an unbounded recurrence.
const MAX_EVENTS: usize = 10_000;

pub(super) async fn collect_outlook(options: &CollectOptions) -> Result<Vec<OutlookEntry>> {
    // A call Outlook does not answer cannot be stopped from here: until it returns, the next
    // collection waits for it (and gives up when it is stopped) instead of adding another.
    static OUTLOOK: Exclusive = Exclusive::new();
    let blocking_options = *options;
    let events = blocking_until_dropped("Outlook collection", move |cancelled| {
        OUTLOOK
            .run(cancelled, || {
                collect_outlook_blocking(&blocking_options, cancelled)
            })
            .and_then(|events| events)
    })
    .await??;
    Ok(normalize_outlook_events(events, options))
}

/// Reads the calendar; between appointments it stops once `cancelled` is set.
fn collect_outlook_blocking(
    options: &CollectOptions,
    cancelled: &AtomicBool,
) -> Result<Vec<RawOutlookEvent>> {
    let _com = ComGuard::new()?;
    let outlook = attach_to_outlook()?;
    let namespace = outlook.method_dispatch_string("GetNamespace", "MAPI")?;
    let calendar = namespace.method_dispatch_i32("GetDefaultFolder", 9)?;
    let items = calendar.property_dispatch("Items")?;
    // Outlook expands recurring meetings only in a collection sorted by start before
    // `IncludeRecurrences` is set (and then restricted), so the order matters. The expanded
    // collection has no usable `Count` (Outlook can report `i32::MAX`), so it is walked with
    // `GetFirst`/`GetNext`.
    items.method_unit_string("Sort", "[Start]")?;
    items.set_bool("IncludeRecurrences", true)?;

    // Restrict compares in Outlook's local time. Rather than converting, pad the UTC range by a
    // day on each side (more than any UTC offset); `normalize_outlook_events` then applies the
    // exact range to the UTC start and end read below.
    let filter = format!(
        "[End] >= '{start}' AND [Start] <= '{end}'",
        start = format_outlook_filter_value(options.start - Days::new(1)),
        end = format_outlook_filter_value(options.end + Days::new(1)),
    );
    let filtered = items.method_dispatch_string("Restrict", &filter)?;
    let mut events = Vec::new();
    let mut next = filtered.method_optional_dispatch("GetFirst")?;

    while let Some(event) = next {
        if cancelled.load(Ordering::Relaxed) {
            return Err(anyhow::Error::new(Stopped));
        }
        if events.len() >= MAX_EVENTS {
            return Err(anyhow::anyhow!(
                "Outlook returned more than {MAX_EVENTS} events for the range; stopped reading"
            ));
        }
        events.push(RawOutlookEvent {
            id: event
                .property_optional_string("GlobalAppointmentID")
                .ok()
                .flatten()
                .filter(|id| !id.is_empty()),
            subject: event.property_optional_string("Subject")?,
            start: event.property_utc_rfc3339("StartUTC")?,
            end: event.property_utc_rfc3339("EndUTC")?,
            location: event.property_optional_string("Location")?,
            organizer: event.property_optional_string("Organizer")?,
            is_organizer: is_organizer_from_com(event.property_i32("MeetingStatus").ok()),
            is_default_calendar: Some(true),
            busy_status: event
                .property_i32("BusyStatus")
                .ok()
                .and_then(busy_status_from_com),
            // Guarded by Outlook's Object Model Guard, like `Organizer`: without healthy
            // antivirus Outlook can ask the user first. A denied or failed read only loses the
            // description.
            body: event.property_optional_string("Body").ok().flatten(),
        });
        next = filtered.method_optional_dispatch("GetNext")?;
    }

    Ok(events)
}

/// Attaches to the Outlook the user has open. Never starts Outlook: a headless instance often has
/// no connection, and starting one would open a full Outlook window.
fn attach_to_outlook() -> Result<DispatchObject> {
    if let Some(outlook) = active_outlook_application()? {
        return Ok(outlook);
    }
    let names = running_process_names();
    let fallback = OutlookFallback::decide(names.iter().map(String::as_str), process_is_elevated());
    match fallback.problem() {
        None => connect_outlook_application().map_err(|err| {
            anyhow::anyhow!(
                "{}: {err:#}. If Outlook runs as administrator, start it normally.",
                fallback.message()
            )
        }),
        Some(problem) => Err(SourceFailure::new(problem, fallback.message()).into()),
    }
}

fn format_outlook_filter_value(value: DateTime<Utc>) -> String {
    value.format("%-m/%-d/%Y %-I:%M %p").to_string()
}
