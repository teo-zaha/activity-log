use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// How the OS store answered, without any key read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub enum OsStoreState {
    Ready,
    /// It is there but would not answer now (interaction not allowed, a dismissed prompt, a
    /// failing call): saving a secret is refused until it does.
    Refused,
}
