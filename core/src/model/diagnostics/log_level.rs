use serde::{Deserialize, Serialize};
use strum::{AsRefStr, Display};
use ts_rs::TS;

#[derive(
    Debug,
    Clone,
    Copy,
    Serialize,
    Deserialize,
    TS,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Display,
    AsRefStr,
)]
#[serde(rename_all = "lowercase")]
#[ts(export, rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}
