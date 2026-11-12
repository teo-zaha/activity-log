use anyhow::{Result, anyhow};
use chrono::{DateTime, TimeZone, Utc};

use crate::model::{CollectOptions, SupportedOs, SystemEntry};
#[cfg(any(target_os = "windows", test))]
use crate::model::{SystemEntryType, system::raw_system_event::RawSystemEvent};

#[cfg(any(target_os = "macos", test))]
mod macos;
#[cfg(test)]
mod tests;
#[cfg(any(target_os = "windows", test))]
mod windows;

#[cfg(any(target_os = "windows", test))]
pub(crate) const WINDOWS_SYSTEM_EVENTS: &[(i32, SystemEntryType, &str)] = &[
    (6005, SystemEntryType::Boot, "System started"),
    (6006, SystemEntryType::Shutdown, "System shutdown"),
    (
        6008,
        SystemEntryType::UnexpectedShutdown,
        "Unexpected shutdown",
    ),
];

#[cfg(target_os = "windows")]
use self::windows as windows_impl;
#[cfg(not(target_os = "windows"))]
mod windows_impl {
    use anyhow::Result;

    use crate::{
        model::{CollectOptions, SystemEntry},
        platform::unsupported_result,
    };

    pub(super) async fn collect_system(_options: &CollectOptions) -> Result<Vec<SystemEntry>> {
        unsupported_result("System collection is not supported on this OS")
    }
}

#[cfg(target_os = "macos")]
use self::macos as macos_impl;
#[cfg(not(target_os = "macos"))]
mod macos_impl {
    use anyhow::Result;

    use crate::{
        model::{CollectOptions, SystemEntry},
        platform::unsupported_result,
    };

    pub(super) async fn collect_system(_options: &CollectOptions) -> Result<Vec<SystemEntry>> {
        unsupported_result("System collection is not supported on this OS")
    }
}

mod linux_impl {
    use anyhow::Result;

    use crate::{
        model::{CollectOptions, SystemEntry},
        platform::unsupported_result,
    };

    pub(super) async fn collect_system(_options: &CollectOptions) -> Result<Vec<SystemEntry>> {
        unsupported_result("System collection is not supported on this OS")
    }
}

pub async fn collect_system(
    options: &CollectOptions,
    current_os: SupportedOs,
) -> Result<Vec<SystemEntry>> {
    match current_os {
        SupportedOs::Windows => windows_impl::collect_system(options).await,
        SupportedOs::Macos => macos_impl::collect_system(options).await,
        SupportedOs::Linux => linux_impl::collect_system(options).await,
    }
}

#[cfg(any(target_os = "windows", test))]
pub(crate) fn map_windows_events(
    raw: &[RawSystemEvent],
    mapping: &[(i32, SystemEntryType, &str)],
) -> Vec<SystemEntry> {
    raw.iter()
        .map(|event| {
            let (_, entry_type, label) = mapping
                .iter()
                .find(|(id, _, _)| *id == event.id)
                .cloned()
                .unwrap_or((event.id, SystemEntryType::Boot, "Unknown event"));

            SystemEntry {
                source: "system".to_string(),
                entry_type,
                timestamp: event.ts.clone(),
                label: if label == "Unknown event" {
                    format!("Event {}", event.id)
                } else {
                    label.to_string()
                },
            }
        })
        .collect()
}

pub(crate) fn utc_bounds(
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
