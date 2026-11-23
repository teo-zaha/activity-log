use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Why the key file holds the key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub enum KeyFileReason {
    /// This system has no OS store this front end can use.
    NoOsStore,
    /// `ACTIVITY_LOG_SECRET_STORE=file`.
    Forced,
    /// A development build on the dev profile.
    DevProfile,
}
