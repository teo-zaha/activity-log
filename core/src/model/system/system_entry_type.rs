use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SystemEntryType {
    Boot,
    Shutdown,
    Sleep,
    Wake,
    UnexpectedShutdown,
    Lock,
    Unlock,
}
