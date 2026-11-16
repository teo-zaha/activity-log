use anyhow::Result;
use chrono::{DateTime, Days, Utc};

use crate::{
    model::{CollectOptions, OutlookEntry, outlook::raw_outlook_event::RawOutlookEvent},
    platform::{
        ComGuard, DispatchObject, active_outlook_application, connect_outlook_application,
        process_is_elevated, running_process_names,
    },
};

use super::{OutlookFallback, normalize_outlook_events};

pub(super) async fn collect_outlook(options: &CollectOptions) -> Result<Vec<OutlookEntry>> {
    let blocking_options = *options;
    let events = tokio::task::spawn_blocking(move || collect_outlook_blocking(&blocking_options))
        .await
        .map_err(|err| anyhow::anyhow!("Outlook collection task failed: {err}"))??;
    Ok(normalize_outlook_events(events, options))
}

fn collect_outlook_blocking(options: &CollectOptions) -> Result<Vec<RawOutlookEvent>> {
    let _com = ComGuard::new()?;
    let outlook = attach_to_outlook()?;
    let namespace = outlook.method_dispatch_string("GetNamespace", "MAPI")?;
    let calendar = namespace.method_dispatch_i32("GetDefaultFolder", 9)?;
    let items = calendar.property_dispatch("Items")?;
    items.set_bool("IncludeRecurrences", true)?;
    items.method_unit_string("Sort", "[Start]")?;

    // Restrict compares in Outlook's local time. Rather than converting, pad the UTC range by a
    // day on each side (more than any UTC offset); `normalize_outlook_events` then applies the
    // exact range to the UTC start and end read below.
    let filter = format!(
        "[End] >= '{start}' AND [Start] <= '{end}'",
        start = format_outlook_filter_value(options.start - Days::new(1)),
        end = format_outlook_filter_value(options.end + Days::new(1)),
    );
    let filtered = items.method_dispatch_string("Restrict", &filter)?;
    let count = filtered.property_i32("Count")?;
    let mut events = Vec::with_capacity(count.max(0) as usize);

    for index in 1..=count {
        let event = filtered.method_dispatch_i32("Item", index)?;
        let meeting_status = event.property_i32("MeetingStatus").unwrap_or_default();
        events.push(RawOutlookEvent {
            subject: event.property_optional_string("Subject")?,
            start: event.property_utc_rfc3339("StartUTC")?,
            end: event.property_utc_rfc3339("EndUTC")?,
            location: event.property_optional_string("Location")?,
            organizer: event.property_optional_string("Organizer")?,
            is_organizer: Some(matches!(meeting_status, 0 | 1)),
            is_default_calendar: Some(true),
        });
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
    match OutlookFallback::decide(names.iter().map(String::as_str), process_is_elevated()) {
        OutlookFallback::Connect => connect_outlook_application().map_err(|err| {
            anyhow::anyhow!(
                "{}: {err:#}. If Outlook runs as administrator, start it normally.",
                OutlookFallback::Connect.message()
            )
        }),
        other => Err(anyhow::anyhow!(other.message())),
    }
}

fn format_outlook_filter_value(value: DateTime<Utc>) -> String {
    value.format("%-m/%-d/%Y %-I:%M %p").to_string()
}
