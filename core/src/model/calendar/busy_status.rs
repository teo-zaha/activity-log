use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// How a meeting shows in the owner's free/busy time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, rename_all = "snake_case")]
pub enum BusyStatus {
    Busy,
    Tentative,
    Free,
    OutOfOffice,
    WorkingElsewhere,
}
