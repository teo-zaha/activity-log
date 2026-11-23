//! Which store a front end keeps the key in (`docs/adr/0006-secure-secrets.md`): the desktop
//! app only the OS store, the CLI the OS store or, where there is none, the key file, the API
//! server what the CLI would, the dev profile always the key file. `ACTIVITY_LOG_SECRET_STORE`
//! chooses where the key lives and configures no source.

use std::fmt;

use super::StoreErrorKind;
use crate::{
    model::{KeyFileReason, NoStoreReason, OsStoreState, SecretStoreInfo},
    profile::Profile,
};

#[cfg(test)]
mod tests;

pub const SECRET_STORE_VARIABLE: &str = "ACTIVITY_LOG_SECRET_STORE";

/// Which program a [`Secrets`](super::Secrets) serves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontEnd {
    Desktop,
    Cli,
    /// It never writes the config, so it never seals; it reports what the CLI would use.
    Api,
}

/// What `ACTIVITY_LOG_SECRET_STORE` asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForcedStore {
    /// The OS store only: no fallback to the key file.
    Os,
    /// The key file, where an OS store exists but cannot be used (over SSH, on a CI runner).
    File,
}

/// A value of the variable that names no store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownStore(pub String);

impl fmt::Display for UnknownStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{SECRET_STORE_VARIABLE}={:?} names no store (`os` or `file`); it is ignored",
            self.0
        )
    }
}

impl ForcedStore {
    /// The variable's value: unset or empty asks for nothing.
    pub fn parse(value: Option<&str>) -> Result<Option<Self>, UnknownStore> {
        match value.filter(|value| !value.is_empty()) {
            None => Ok(None),
            Some("os") => Ok(Some(Self::Os)),
            Some("file") => Ok(Some(Self::File)),
            Some(other) => Err(UnknownStore(other.to_string())),
        }
    }
}

/// What decides the store, before any store is asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Choice {
    pub front_end: FrontEnd,
    pub profile: Profile,
    /// What the variable asks for; the desktop app never reads it.
    pub forced: Option<ForcedStore>,
    /// Whether this program has an OS store at all (none in the dev profile and in tests).
    pub has_os_store: bool,
}

/// How the OS store answered its probe, if it was asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OsAnswer {
    NotAsked,
    Ready,
    Error(StoreErrorKind),
}

/// Where the key is decided to live, before the OS store is asked or with its answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Decided {
    /// The key file, whatever the OS store says.
    File(KeyFileReason),
    /// No store at all, whatever the OS store says.
    Nothing(NoStoreReason),
    /// The OS store, or, if it is unavailable, what [`Self::os_unavailable`] says.
    OsFirst,
}

impl Choice {
    pub(crate) fn decided(self) -> Decided {
        match (self.profile, self.forced, self.front_end) {
            (Profile::Dev, _, _) => Decided::File(KeyFileReason::DevProfile),
            (Profile::Installed, Some(ForcedStore::File), FrontEnd::Cli | FrontEnd::Api) => {
                Decided::File(KeyFileReason::Forced)
            }
            _ if !self.has_os_store => self.os_unavailable(),
            _ => Decided::OsFirst,
        }
    }

    /// Where the key lives when the OS store is unavailable: the desktop app has nothing (it
    /// seals only with the OS store), and neither does a CLI the variable holds to it; the CLI
    /// and the API server fall back to the key file.
    pub(crate) fn os_unavailable(self) -> Decided {
        match (self.front_end, self.forced) {
            (FrontEnd::Desktop, _) | (_, Some(ForcedStore::Os)) => {
                Decided::Nothing(NoStoreReason::NoOsStore)
            }
            (FrontEnd::Cli | FrontEnd::Api, _) => Decided::File(KeyFileReason::NoOsStore),
        }
    }

    /// What the settings show, from the OS store's answer. Until it is asked it is shown as
    /// ready, a probe at start following at once.
    pub fn info(self, os: OsAnswer) -> SecretStoreInfo {
        let decided = match (self.decided(), os) {
            (Decided::OsFirst, OsAnswer::Error(StoreErrorKind::Unavailable)) => {
                self.os_unavailable()
            }
            (decided, _) => decided,
        };
        match (decided, os) {
            (Decided::File(why), _) => SecretStoreInfo::File { why },
            (Decided::Nothing(why), _) => SecretStoreInfo::Unavailable { why },
            (Decided::OsFirst, OsAnswer::NotAsked | OsAnswer::Ready) => SecretStoreInfo::Os {
                state: OsStoreState::Ready,
            },
            (Decided::OsFirst, OsAnswer::Error(_)) => SecretStoreInfo::Os {
                state: OsStoreState::Refused,
            },
        }
    }
}
