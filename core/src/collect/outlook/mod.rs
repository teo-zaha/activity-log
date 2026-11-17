use anyhow::Result;
use chrono::{DateTime, Utc};
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

/// What to do when Outlook has not registered itself for other apps to attach to.
#[cfg(any(target_os = "windows", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OutlookFallback {
    /// Classic Outlook runs: ask COM for its application object. Outlook is single-instance, so
    /// this returns the open window's instance instead of starting a second one.
    Connect,
    /// Outlook runs, but this app is elevated and Windows keeps it from reaching Outlook.
    Elevated,
    /// Only the new Outlook runs, which has no COM interface.
    NewOutlookOnly,
    NotRunning,
}

#[cfg(any(target_os = "windows", test))]
impl OutlookFallback {
    /// Decides from the executable names of the running processes. Outlook often registers its
    /// running object late or not at all (it is per integrity level, too), so a failed lookup
    /// alone does not mean it is closed.
    pub(crate) fn decide<'a>(
        process_names: impl IntoIterator<Item = &'a str>,
        elevated: bool,
    ) -> Self {
        let (mut classic, mut new) = (false, false);
        for name in process_names {
            classic |= name.eq_ignore_ascii_case("outlook.exe");
            new |= name.eq_ignore_ascii_case("olk.exe");
        }
        match (classic, new) {
            (true, _) if elevated => Self::Elevated,
            (true, _) => Self::Connect,
            (false, true) => Self::NewOutlookOnly,
            (false, false) => Self::NotRunning,
        }
    }

    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::Connect => "Outlook is running but could not be reached",
            Self::Elevated => {
                "Outlook is open, but Activity Log runs as administrator and Windows keeps it from reaching Outlook. Start Activity Log without administrator rights."
            }
            Self::NewOutlookOnly => {
                "Only the new Outlook is running. It offers no interface for reading the calendar; switch to classic Outlook to include it."
            }
            Self::NotRunning => "Outlook is not running. Open Outlook to include its calendar.",
        }
    }
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
    events
        .into_iter()
        .filter(default_calendar_event)
        .filter(|event| event_overlaps_range(event, options))
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
        connection: String::new(),
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

fn event_overlaps_range(event: &RawOutlookEvent, options: &CollectOptions) -> bool {
    let instant = |raw: &str| DateTime::parse_from_rfc3339(raw).map(|at| at.with_timezone(&Utc));
    match (instant(&event.start), instant(&event.end)) {
        (Ok(start), Ok(end)) => options.overlaps(start, end),
        _ => false,
    }
}
