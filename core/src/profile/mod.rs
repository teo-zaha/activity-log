//! Which folders and which secret store a process uses: the installed ones, or for a
//! development build the dev profile's own (`docs/adr/0006-secure-secrets.md`).
//!
//! `ACTIVITY_LOG_PROFILE=dev`, which the development scripts set, gives a debug build its own
//! config, cache and key file under `activity-log-dev`, and the key file as its only store, so
//! a rebuild never asks the keychain and nothing is sealed into the real one. A release build
//! (the installed app and CLI) ignores the variable and says so once: a stray value can never
//! stop it from starting. An unknown value in a debug build is refused at start.

use std::{fmt, path::Path, sync::OnceLock};

use crate::diagnostics;

#[cfg(test)]
mod tests;

pub const PROFILE_VARIABLE: &str = "ACTIVITY_LOG_PROFILE";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    Installed,
    Dev,
}

impl Profile {
    /// The folder this profile keeps its files in, under each OS folder.
    pub const fn folder(self) -> &'static str {
        match self {
            Self::Installed => "activity-log",
            Self::Dev => "activity-log-dev",
        }
    }
}

/// How this build read the variable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Chosen {
    Profile(Profile),
    /// A release build found the variable set and goes on as installed.
    IgnoredInRelease,
}

/// A debug build was asked for a profile there is none of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownProfile(String);

impl fmt::Display for UnknownProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{PROFILE_VARIABLE}={:?} names no profile: the only one is `dev`",
            self.0
        )
    }
}

impl std::error::Error for UnknownProfile {}

/// How a build reads the variable: unset or empty is the installed profile; a release build
/// ignores any value; a debug build takes `dev` and refuses anything else.
fn choose(value: Option<&str>, debug_build: bool) -> Result<Chosen, UnknownProfile> {
    match value.filter(|value| !value.is_empty()) {
        None => Ok(Chosen::Profile(Profile::Installed)),
        Some(_) if !debug_build => Ok(Chosen::IgnoredInRelease),
        Some("dev") => Ok(Chosen::Profile(Profile::Dev)),
        Some(other) => Err(UnknownProfile(other.to_string())),
    }
}

static PROFILE: OnceLock<Profile> = OnceLock::new();

/// Reads the variable once for the process, before anything else resolves a path; each binary
/// calls it first and stops on an error. It says which profile it chose once: the dev profile
/// with where its settings and its key are (`explicit_config` is `--config`, which still
/// chooses the settings file), or that a release build ignored the variable.
pub fn init(explicit_config: Option<&Path>) -> Result<Profile, UnknownProfile> {
    let value = std::env::var(PROFILE_VARIABLE).ok();
    let chosen = choose(value.as_deref(), cfg!(debug_assertions))?;
    let profile = match chosen {
        Chosen::Profile(profile) => profile,
        Chosen::IgnoredInRelease => Profile::Installed,
    };
    let profile = *PROFILE.get_or_init(|| profile);
    match (chosen, profile) {
        (Chosen::IgnoredInRelease, _) => diagnostics::warn(
            "profile",
            format!("{PROFILE_VARIABLE} is ignored: an installed build has no profiles"),
        ),
        (Chosen::Profile(_), Profile::Dev) => diagnostics::warn(
            "profile",
            format!(
                "Using the dev profile: settings in {}, secrets in a key file at {}",
                shown(crate::settings::ConfigFile::resolve_path(explicit_config).as_deref()),
                shown(crate::secrets::KeyFileStore::default_path().as_deref()),
            ),
        ),
        (Chosen::Profile(_), Profile::Installed) => {}
    }
    Ok(profile)
}

/// The profile [`init`] chose; the installed one where nothing chose (core's own tests, a
/// program that never calls it).
pub fn current() -> Profile {
    PROFILE.get().copied().unwrap_or(Profile::Installed)
}

fn shown(path: Option<&Path>) -> String {
    path.map_or_else(
        || "no folder".to_string(),
        |path| path.display().to_string(),
    )
}
