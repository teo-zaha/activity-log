use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Why an import that brings a token waits for a later start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub enum LegacyStoreWait {
    /// No secure store this front end seals with.
    Unavailable,
    /// The store refused, or failed.
    Refused,
}
