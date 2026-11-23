use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::common::{
    key_file_reason::KeyFileReason, no_store_reason::NoStoreReason, os_store_state::OsStoreState,
};

/// Where this front end keeps the key new secrets are sealed with, as the settings show it.
/// Never a key, never a secret.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "store", rename_all = "camelCase")]
#[ts(export, tag = "store", rename_all = "camelCase")]
pub enum SecretStoreInfo {
    /// The OS secure store: the keychain, Credential Manager, the Secret Service.
    Os { state: OsStoreState },
    /// The key file, and why it is used instead of the OS store.
    File { why: KeyFileReason },
    /// No store: the desktop app without an OS store, which seals with nothing else.
    Unavailable { why: NoStoreReason },
}
