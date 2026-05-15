use anyhow::{Result, anyhow};
use serde_json::Value;

use crate::model::{
    CollectOptions, OutlookEntry, SupportedOs, outlook::raw_outlook_event::RawOutlookEvent,
};

mod macos;
#[cfg(test)]
mod tests;
mod windows;

pub async fn collect_outlook(
    options: &CollectOptions,
    current_os: SupportedOs,
) -> Result<Vec<OutlookEntry>> {
    match current_os {
        SupportedOs::Windows => windows::collect_outlook(options).await,
        SupportedOs::Macos => macos::collect_outlook(options).await,
        SupportedOs::Linux => Err(anyhow!("Outlook collection is not supported on this OS")),
    }
}

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

pub(crate) fn normalize_outlook_events(events: Vec<RawOutlookEvent>) -> Vec<OutlookEntry> {
    events.into_iter().map(normalize_outlook_event).collect()
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
