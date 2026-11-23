//! The one item of the OS secure store that holds the config key: the login keychain on
//! macOS, Credential Manager on Windows, the Secret Service on Linux.
//!
//! Each answer is definite: [`Lookup::Absent`] only when the store answered, is usable and
//! holds no such item. A store that is missing, locked, refusing or failing says so with its
//! [`StoreErrorKind`], and nothing is ever created over it.

use std::path::PathBuf;

use crate::secrets::{Created, DataKey, Interaction, Lookup, StoreError, StoreErrorKind};

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
mod fallback;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
use fallback as imp;
#[cfg(target_os = "linux")]
use linux as imp;
#[cfg(target_os = "macos")]
use macos as imp;
#[cfg(target_os = "windows")]
use windows as imp;

#[cfg(test)]
mod tests;

/// Which item: the config key, or the real-store tests' own, so that they never touch the
/// user's key even where they run in the user's store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ItemName {
    #[cfg_attr(test, allow(dead_code))]
    ConfigKey,
    #[cfg_attr(not(feature = "real-store-tests"), allow(dead_code))]
    Test,
}

/// Where the item is: its name, and on macOS the keychain file the tests use instead of the
/// user's default keychain.
#[derive(Debug, Clone)]
pub(crate) struct Place {
    pub(crate) name: ItemName,
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub(crate) keychain: Option<PathBuf>,
}

impl Place {
    #[cfg_attr(test, allow(dead_code))]
    pub(crate) fn config_key() -> Self {
        Self {
            name: ItemName::ConfigKey,
            keychain: None,
        }
    }
}

pub(crate) async fn read(place: &Place, interaction: Interaction) -> Result<Lookup, StoreError> {
    imp::read(place.clone(), interaction).await
}

/// Stores `key` unless the item exists. Called under the key-creation lock.
pub(crate) async fn create_if_absent(place: &Place, key: &DataKey) -> Result<Created, StoreError> {
    imp::create_if_absent(place.clone(), key.clone()).await
}

/// Whether the store is there and answering, without reading the item or showing a dialog.
pub(crate) async fn probe(place: &Place) -> Result<(), StoreError> {
    imp::probe(place.clone()).await
}

/// Removes the item; only the real-store tests do.
#[cfg(feature = "real-store-tests")]
pub(crate) async fn delete(place: &Place) -> Result<(), StoreError> {
    imp::delete(place.clone()).await
}

/// What a store's status code says about a read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    not(any(target_os = "macos", target_os = "windows", test)),
    allow(dead_code)
)]
enum Status {
    Absent,
    Duplicate,
    Error(StoreErrorKind),
}

/// A Security framework `OSStatus` other than success.
#[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
fn macos_status(code: i32) -> Status {
    const USER_CANCELED: i32 = -128;
    const NOT_AVAILABLE: i32 = -25291;
    const AUTH_FAILED: i32 = -25293;
    const NO_SUCH_KEYCHAIN: i32 = -25294;
    const DUPLICATE_ITEM: i32 = -25299;
    const ITEM_NOT_FOUND: i32 = -25300;
    const NO_DEFAULT_KEYCHAIN: i32 = -25307;
    const INTERACTION_NOT_ALLOWED: i32 = -25308;
    const INTERACTION_REQUIRED: i32 = -25315;
    match code {
        ITEM_NOT_FOUND => Status::Absent,
        DUPLICATE_ITEM => Status::Duplicate,
        NO_DEFAULT_KEYCHAIN | NOT_AVAILABLE | NO_SUCH_KEYCHAIN => {
            Status::Error(StoreErrorKind::Unavailable)
        }
        INTERACTION_NOT_ALLOWED | INTERACTION_REQUIRED | AUTH_FAILED | USER_CANCELED => {
            Status::Error(StoreErrorKind::Refused)
        }
        _ => Status::Error(StoreErrorKind::Failed),
    }
}

/// A Win32 error code from the credential functions.
#[cfg_attr(not(any(target_os = "windows", test)), allow(dead_code))]
fn windows_status(code: u32) -> Status {
    const ERROR_NOT_FOUND: u32 = 1168;
    const ERROR_NO_SUCH_LOGON_SESSION: u32 = 1312;
    match code {
        ERROR_NOT_FOUND => Status::Absent,
        ERROR_NO_SUCH_LOGON_SESSION => Status::Error(StoreErrorKind::Unavailable),
        _ => Status::Error(StoreErrorKind::Failed),
    }
}

/// The key an item holds: exactly a key's bytes, or a store that failed.
#[cfg_attr(
    not(any(target_os = "macos", target_os = "windows", target_os = "linux", test)),
    allow(dead_code)
)]
fn key_of(bytes: &[u8]) -> Result<Lookup, StoreError> {
    DataKey::from_slice(bytes)
        .map(Lookup::Found)
        .ok_or_else(|| {
            StoreError::failed(anyhow::anyhow!(
                "The config key item holds {} bytes, not {}",
                bytes.len(),
                crate::secrets::KEY_LEN
            ))
        })
}
