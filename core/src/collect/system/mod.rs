use anyhow::Result;

use crate::model::{CollectOptions, SystemEntry};
#[cfg(any(target_os = "windows", test))]
use crate::model::{
    SystemEntryType, SystemTag, Timestamp, system::raw_system_event::RawSystemEvent,
};

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

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
use self::fallback as imp;
#[cfg(target_os = "macos")]
use self::macos as imp;
#[cfg(target_os = "windows")]
use self::windows as imp;

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
mod fallback {
    use anyhow::{Result, bail};

    use crate::model::{CollectOptions, SystemEntry};

    pub(super) async fn collect_system(_options: &CollectOptions) -> Result<Vec<SystemEntry>> {
        bail!("System collection is not supported on this OS")
    }
}

pub async fn collect_system(options: &CollectOptions) -> Result<Vec<SystemEntry>> {
    imp::collect_system(options).await
}

#[cfg(any(target_os = "windows", test))]
pub(crate) fn map_windows_events(
    raw: &[RawSystemEvent],
    mapping: &[(i32, SystemEntryType, &str)],
) -> Vec<SystemEntry> {
    raw.iter()
        .filter_map(|event| {
            // The query asks only for mapped ids; any other is no boot or shutdown to show.
            let (_, entry_type, label) = mapping.iter().find(|(id, _, _)| *id == event.id)?;
            Some(SystemEntry {
                source: SystemTag,
                connection: crate::model::ConnectionName::unstamped(),
                id: format!("{entry_type}/{}", event.ts),
                entry_type: entry_type.clone(),
                timestamp: Timestamp::parse(event.ts.as_str()).ok()?,
                label: label.to_string(),
            })
        })
        .collect()
}
