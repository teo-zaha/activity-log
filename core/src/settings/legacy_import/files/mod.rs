//! Where 0.0.7 kept its settings, and reading them: the desktop's Tauri Store `settings.json`,
//! and the `.env` the CLI and the API loaded, found as 0.0.7 found it (the first regular file
//! named `.env` walking up from the working directory) but taken only when the user owns it.
//! Nothing here writes, moves or deletes a legacy file, and no value reaches a message.

use std::{
    io,
    path::{Path, PathBuf},
};

use super::v0_0_7::{LegacyValues, StoreRefusal, read_store_provider_config};
pub use crate::model::LegacySource;
use crate::platform::{dirs, private_file};

#[cfg(test)]
mod tests;

/// 0.0.7's desktop identifier, its store's folder name.
const DESKTOP_IDENTIFIER: &str = "com.activitylog.desktop";
const SETTINGS_JSON: &str = "settings.json";
const STORE_KEY: &str = "providerConfig";
const ENV: &str = ".env";

pub(super) fn file_name(source: LegacySource) -> &'static str {
    match source {
        LegacySource::Settings => SETTINGS_JSON,
        LegacySource::Env => ENV,
    }
}

/// One legacy file where it was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LegacyFile {
    pub(super) source: LegacySource,
    pub(super) path: PathBuf,
}

impl std::fmt::Display for LegacyFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} at {}", file_name(self.source), self.path.display())
    }
}

/// Why a legacy file gave nothing. Never holds a value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unreadable {
    Io(io::ErrorKind),
    /// `settings.json` is not JSON, or not an object.
    NotJson,
    /// 0.0.7 refused its `providerConfig` as a whole.
    Refused(StoreRefusal),
}

impl std::fmt::Display for Unreadable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(kind) => write!(f, "it could not be read ({kind})"),
            Self::NotJson => write!(f, "it is not a JSON object"),
            Self::Refused(refusal) => write!(f, "{refusal}"),
        }
    }
}

/// What a legacy file gave.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Read {
    Values {
        values: LegacyValues,
        /// A `.env` line did not parse: 0.0.7 loaded the lines before it and none after, and
        /// so does the import.
        stopped: bool,
    },
    Unreadable(Unreadable),
}

/// Where 0.0.7's desktop kept `settings.json`: Tauri's app data folder for its identifier.
pub(super) fn settings_json_path() -> Option<PathBuf> {
    let home = if cfg!(target_os = "linux") {
        dirs::local_data_home()
    } else {
        dirs::config_home()
    };
    home.map(|home| home.join(DESKTOP_IDENTIFIER).join(SETTINGS_JSON))
}

/// What a walk for `.env` found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum EnvFound {
    None,
    Found(PathBuf),
    /// The first `.env` up the walk belongs to another account (or its owner cannot be told):
    /// it is not read, and the walk stops there, as 0.0.7's would have.
    NotOwned(PathBuf),
}

/// The first regular file named `.env` from `start` up to the root, a link followed.
pub(super) fn find_env(start: &Path) -> EnvFound {
    let Some(path) = start
        .ancestors()
        .map(|folder| folder.join(ENV))
        .find(|candidate| candidate.is_file())
    else {
        return EnvFound::None;
    };
    match private_file::owned_by_user(&path) {
        Ok(true) => EnvFound::Found(path),
        Ok(false) | Err(_) => EnvFound::NotOwned(path),
    }
}

/// `settings.json` as 0.0.7's desktop read it; `None` when there is none.
pub(super) fn read_settings(path: &Path) -> Option<Read> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return None,
        Err(err) => return Some(Read::Unreadable(Unreadable::Io(err.kind()))),
    };
    let Ok(serde_json::Value::Object(store)) = serde_json::from_slice(&bytes) else {
        return Some(Read::Unreadable(Unreadable::NotJson));
    };
    Some(match read_store_provider_config(store.get(STORE_KEY)) {
        Ok(values) => Read::Values {
            values: LegacyValues::new(values),
            stopped: false,
        },
        Err(refusal) => Read::Unreadable(Unreadable::Refused(refusal)),
    })
}

/// A `.env` as text pairs, the first of a repeated key winning, as 0.0.7's `dotenv()` set them
/// (it never replaced a variable it had set). Parsed by `dotenvy`, the parser 0.0.7 used,
/// without touching the process environment.
pub(super) fn read_env(path: &Path) -> Read {
    let lines = match dotenvy::from_path_iter(path) {
        Ok(lines) => lines,
        Err(err) => return Read::Unreadable(Unreadable::Io(io_kind(&err))),
    };
    let mut values = std::collections::BTreeMap::new();
    let mut stopped = false;
    for line in lines {
        match line {
            Ok((key, value)) => {
                values.entry(key).or_insert(Some(value));
            }
            // Its text may hold a value: only that it stopped is kept.
            Err(dotenvy::Error::LineParse(..)) => {
                stopped = true;
                break;
            }
            Err(err) => return Read::Unreadable(Unreadable::Io(io_kind(&err))),
        }
    }
    Read::Values {
        values: LegacyValues::new(values),
        stopped,
    }
}

fn io_kind(err: &dotenvy::Error) -> io::ErrorKind {
    match err {
        dotenvy::Error::Io(err) => err.kind(),
        _ => io::ErrorKind::InvalidData,
    }
}
