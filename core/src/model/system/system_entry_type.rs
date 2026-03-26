use serde::{Deserialize, Serialize};
use strum::{AsRefStr, Display, EnumString};
use ts_rs::TS;

#[derive(
    Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq, Display, AsRefStr, EnumString,
)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
#[strum(serialize_all = "snake_case", ascii_case_insensitive)]
pub enum SystemEntryType {
    Boot,
    Shutdown,
    Sleep,
    Wake,
    UnexpectedShutdown,
    Lock,
    Unlock,
}
