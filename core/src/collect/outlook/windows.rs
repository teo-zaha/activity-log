use anyhow::Result;

use crate::{
    model::{CollectOptions, OutlookEntry, outlook::raw_outlook_event::RawOutlookEvent},
    platform::{ComGuard, local_date_bounds_as_systemtime, running_outlook_application},
};

use super::normalize_outlook_events;

pub(super) async fn collect_outlook(options: &CollectOptions) -> Result<Vec<OutlookEntry>> {
    let blocking_options = options.clone();
    let events = tokio::task::spawn_blocking(move || collect_outlook_blocking(&blocking_options))
        .await
        .map_err(|err| anyhow::anyhow!("Outlook collection task failed: {err}"))??;
    Ok(normalize_outlook_events(events, options))
}

fn collect_outlook_blocking(options: &CollectOptions) -> Result<Vec<RawOutlookEvent>> {
    let _com = ComGuard::new()?;
    let outlook = running_outlook_application()?;
    let namespace = outlook.method_dispatch_string("GetNamespace", "MAPI")?;
    let calendar = namespace.method_dispatch_i32("GetDefaultFolder", 9)?;
    let items = calendar.property_dispatch("Items")?;
    items.set_bool("IncludeRecurrences", true)?;
    items.method_unit_string("Sort", "[Start]")?;

    let (start, end) = local_date_bounds_as_systemtime(options.since, options.until)?;
    let filter = format!(
        "[End] >= '{start}' AND [Start] <= '{end}'",
        start = format_outlook_filter_value(&start),
        end = format_outlook_filter_value(&end),
    );
    let filtered = items.method_dispatch_string("Restrict", &filter)?;
    let count = filtered.property_i32("Count")?;
    let mut events = Vec::with_capacity(count.max(0) as usize);

    for index in 1..=count {
        let event = filtered.method_dispatch_i32("Item", index)?;
        let meeting_status = event.property_i32("MeetingStatus").unwrap_or_default();
        events.push(RawOutlookEvent {
            subject: event.property_optional_string("Subject")?,
            start: event.property_rfc3339("Start")?,
            end: event.property_rfc3339("End")?,
            location: event.property_optional_string("Location")?,
            organizer: event.property_optional_string("Organizer")?,
            is_organizer: Some(matches!(meeting_status, 0 | 1)),
            is_default_calendar: Some(true),
        });
    }

    Ok(events)
}

fn format_outlook_filter_value(value: &windows::Win32::Foundation::SYSTEMTIME) -> String {
    let hour24 = value.wHour;
    let hour12 = match hour24 % 12 {
        0 => 12,
        other => other,
    };
    let am_pm = if hour24 < 12 { "AM" } else { "PM" };

    format!(
        "{}/{}/{} {}:{:02} {}",
        value.wMonth, value.wDay, value.wYear, hour12, value.wMinute, am_pm
    )
}
