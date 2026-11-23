//! The OS folders for settings and caches, read from the environment the OS sets.

use std::{env, path::PathBuf};

/// The OS folder for settings: `%APPDATA%`, `~/Library/Application Support` or
/// `$XDG_CONFIG_HOME` (`~/.config`); `None` without one.
pub(crate) fn config_home() -> Option<PathBuf> {
    if cfg!(target_os = "windows") {
        env_path("APPDATA")
    } else if cfg!(target_os = "macos") {
        home().map(|home| home.join("Library").join("Application Support"))
    } else {
        env_path("XDG_CONFIG_HOME").or_else(|| home().map(|home| home.join(".config")))
    }
}

/// The OS folder for caches: `%LOCALAPPDATA%`, `~/Library/Caches` or `$XDG_CACHE_HOME`
/// (`~/.cache`); `None` without one.
pub(crate) fn cache_home() -> Option<PathBuf> {
    if cfg!(target_os = "windows") {
        env_path("LOCALAPPDATA")
    } else if cfg!(target_os = "macos") {
        home().map(|home| home.join("Library").join("Caches"))
    } else {
        env_path("XDG_CACHE_HOME").or_else(|| home().map(|home| home.join(".cache")))
    }
}

/// The OS folder for data that stays on this computer (never roams with a profile):
/// `%LOCALAPPDATA%`, `~/Library/Application Support` or `$XDG_DATA_HOME` (`~/.local/share`);
/// `None` without one.
pub(crate) fn local_data_home() -> Option<PathBuf> {
    if cfg!(target_os = "windows") {
        env_path("LOCALAPPDATA")
    } else if cfg!(target_os = "macos") {
        home().map(|home| home.join("Library").join("Application Support"))
    } else {
        env_path("XDG_DATA_HOME").or_else(|| home().map(|home| home.join(".local").join("share")))
    }
}

fn home() -> Option<PathBuf> {
    env_path("HOME")
}

/// A path from the environment; an empty variable counts as unset.
pub(crate) fn env_path(name: &str) -> Option<PathBuf> {
    env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}
