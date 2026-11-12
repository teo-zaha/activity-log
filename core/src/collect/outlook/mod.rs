use anyhow::{Result, anyhow};
use chrono::{DateTime, TimeZone, Utc};
#[cfg(any(target_os = "macos", test))]
use serde_json::Value;

use crate::model::{
    CollectOptions, OutlookEntry, SupportedOs, outlook::raw_outlook_event::RawOutlookEvent,
};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(test)]
mod tests;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "windows")]
use self::windows as windows_impl;
#[cfg(not(target_os = "windows"))]
mod windows_impl {
    use anyhow::Result;

    use crate::{
        model::{CollectOptions, OutlookEntry},
        platform::unsupported_result,
    };

    pub(super) async fn collect_outlook(_options: &CollectOptions) -> Result<Vec<OutlookEntry>> {
        unsupported_result("Outlook collection is not supported on this OS")
    }
}

#[cfg(target_os = "macos")]
use self::macos as macos_impl;
#[cfg(not(target_os = "macos"))]
mod macos_impl {
    use anyhow::Result;

    use crate::{
        model::{CollectOptions, OutlookEntry},
        platform::unsupported_result,
    };

    pub(super) async fn collect_outlook(_options: &CollectOptions) -> Result<Vec<OutlookEntry>> {
        unsupported_result("Outlook collection is not supported on this OS")
    }
}

mod linux_impl {
    use anyhow::Result;

    use crate::{
        model::{CollectOptions, OutlookEntry},
        platform::unsupported_result,
    };

    pub(super) async fn collect_outlook(_options: &CollectOptions) -> Result<Vec<OutlookEntry>> {
        unsupported_result("Outlook collection is not supported on this OS")
    }
}

pub async fn collect_outlook(
    options: &CollectOptions,
    current_os: SupportedOs,
) -> Result<Vec<OutlookEntry>> {
    match current_os {
        SupportedOs::Windows => windows_impl::collect_outlook(options).await,
        SupportedOs::Macos => macos_impl::collect_outlook(options).await,
        SupportedOs::Linux => linux_impl::collect_outlook(options).await,
    }
}

pub(crate) fn outlook_range_bounds(
    since: chrono::NaiveDate,
    until: chrono::NaiveDate,
) -> Result<(DateTime<Utc>, DateTime<Utc>)> {
    let start = chrono::Local
        .from_local_datetime(
            &since
                .and_hms_opt(0, 0, 0)
                .ok_or_else(|| anyhow!("invalid since date"))?,
        )
        .single()
        .ok_or_else(|| anyhow!("invalid since date"))?
        .with_timezone(&Utc);
    let end = chrono::Local
        .from_local_datetime(
            &until
                .and_hms_opt(23, 59, 59)
                .ok_or_else(|| anyhow!("invalid until date"))?,
        )
        .single()
        .ok_or_else(|| anyhow!("invalid until date"))?
        .with_timezone(&Utc);

    Ok((start, end))
}

#[cfg(any(target_os = "macos", test))]
pub(crate) fn parse_raw_outlook_events(stdout: &str) -> Result<Vec<RawOutlookEvent>> {
    let stdout = stdout.trim();
    if stdout.is_empty() || stdout == "[]" {
        return Ok(Vec::new());
    }

    let parsed = serde_json::from_str::<Value>(stdout)?;
    if let Some(array) = parsed.as_array() {
        array
            .iter()
            .cloned()
            .map(serde_json::from_value)
            .collect::<Result<Vec<RawOutlookEvent>, _>>()
            .map_err(Into::into)
    } else {
        Ok(vec![serde_json::from_value::<RawOutlookEvent>(parsed)?])
    }
}

pub(crate) fn normalize_outlook_events(
    events: Vec<RawOutlookEvent>,
    options: &CollectOptions,
) -> Vec<OutlookEntry> {
    let Ok((start, end)) = outlook_range_bounds(options.since, options.until) else {
        return Vec::new();
    };

    events
        .into_iter()
        .filter(default_calendar_event)
        .filter(|event| event_overlaps_range(event, start, end))
        .map(normalize_outlook_event)
        .collect()
}

fn normalize_outlook_event(event: RawOutlookEvent) -> OutlookEntry {
    let duration_minutes = chrono::DateTime::parse_from_rfc3339(&event.end)
        .and_then(|end| {
            chrono::DateTime::parse_from_rfc3339(&event.start)
                .map(|start| (end - start).num_minutes())
        })
        .unwrap_or_default();

    OutlookEntry {
        source: "outlook".to_string(),
        timestamp: event.start,
        title: event.subject.unwrap_or_else(|| "(No title)".to_string()),
        location: event.location,
        organizer: event.organizer,
        is_organizer: event.is_organizer.unwrap_or(false),
        duration_minutes: duration_minutes as i32,
    }
}

fn default_calendar_event(event: &RawOutlookEvent) -> bool {
    event.is_default_calendar.unwrap_or(true)
}

fn event_overlaps_range(event: &RawOutlookEvent, start: DateTime<Utc>, end: DateTime<Utc>) -> bool {
    let Ok(event_start) = chrono::DateTime::parse_from_rfc3339(&event.start) else {
        return false;
    };
    let Ok(event_end) = chrono::DateTime::parse_from_rfc3339(&event.end) else {
        return false;
    };

    let event_start = event_start.with_timezone(&Utc);
    let event_end = event_end.with_timezone(&Utc);

    event_end >= start && event_start <= end
}
