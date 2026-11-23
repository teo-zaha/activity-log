use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Why no store holds the key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub enum NoStoreReason {
    /// This system has no OS store, and this front end seals with nothing else.
    NoOsStore,
}
