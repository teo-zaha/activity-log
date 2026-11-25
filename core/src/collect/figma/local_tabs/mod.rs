use std::{io, path::Path};

use serde_json::Value;

use crate::{
    platform::private_file::{OwnedInput, read_owned_input},
    provider::figma::scope::FigmaFileKey,
};

#[cfg(test)]
mod tests;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod fallback;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use fallback as imp;
#[cfg(target_os = "macos")]
use macos as imp;
#[cfg(target_os = "windows")]
use windows as imp;

pub(crate) const SUPPORTED: bool = imp::SUPPORTED;

const LIMIT: u32 = 4 * 1024 * 1024;

#[derive(Debug)]
pub(crate) enum Outcome {
    Absent,
    Found(Vec<Tab>),
    TooLarge,
    NotOwned,
    Unrecognized,
    Unreadable(io::Error),
    WorkerStopped(tokio::task::JoinError),
}

impl Outcome {
    pub(super) fn has_snapshot(&self) -> bool {
        matches!(self, Self::Absent | Self::Found(_))
    }

    pub(crate) fn files_for<'a>(
        &'a self,
        principal: &'a str,
    ) -> impl Iterator<Item = &'a FigmaFileKey> {
        let tabs = match self {
            Self::Found(tabs) => tabs.as_slice(),
            _ => &[],
        };
        tabs.iter()
            .filter(move |tab| tab.user_id == principal)
            .map(|tab| &tab.file)
    }

    pub(crate) fn warning(&self) -> Option<String> {
        match self {
            Self::Absent | Self::Found(_) => None,
            Self::TooLarge => Some("Figma's desktop settings exceed the 4 MiB read limit.".into()),
            Self::NotOwned => Some(
                "Figma's desktop settings are writable by another account or do not belong to this user.".into(),
            ),
            Self::Unrecognized => {
                Some("Figma's desktop settings do not contain a recognized tab list.".into())
            }
            Self::Unreadable(error) => Some(format!("Could not read Figma's desktop settings: {error}.")),
            Self::WorkerStopped(error) => {
                let reason = if error.is_panic() { "stopped unexpectedly" } else { "was cancelled" };
                Some(format!("The Figma desktop settings reader {reason}."))
            }
        }
    }
}

#[derive(Debug)]
pub(crate) struct Tab {
    user_id: String,
    file: FigmaFileKey,
}

pub(crate) async fn find() -> Outcome {
    crate::cache::state_file::blocking(|| {
        imp::settings_path()
            .as_deref()
            .map_or(Outcome::Absent, read)
    })
    .await
    .unwrap_or_else(Outcome::WorkerStopped)
}

pub(crate) fn read(path: &Path) -> Outcome {
    match read_owned_input(path, LIMIT) {
        Ok(OwnedInput::Bytes(bytes)) => parse(&bytes),
        Ok(OwnedInput::Absent) => Outcome::Absent,
        Ok(OwnedInput::TooLarge) => Outcome::TooLarge,
        Ok(OwnedInput::NotOwned) => Outcome::NotOwned,
        Err(error) => Outcome::Unreadable(error),
    }
}

fn parse(bytes: &[u8]) -> Outcome {
    let Ok(settings) = serde_json::from_slice::<Value>(bytes) else {
        return Outcome::Unrecognized;
    };
    let Some(tabs) = settings["lastOpenWindow"]["tabs"].as_array() else {
        return Outcome::Unrecognized;
    };
    Outcome::Found(tabs.iter().filter_map(tab).collect())
}

fn tab(row: &Value) -> Option<Tab> {
    let file = FigmaFileKey::from_path(row.get("path")?.as_str()?)?;
    let user_id = match row.get("userId")? {
        Value::String(id) if !id.is_empty() => id.clone(),
        Value::Number(id) => id.as_u64()?.to_string(),
        _ => return None,
    };
    Some(Tab { user_id, file })
}
