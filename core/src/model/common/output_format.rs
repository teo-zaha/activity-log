use serde::{Deserialize, Serialize};
use strum::{AsRefStr, Display, EnumString};
use ts_rs::TS;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Display, AsRefStr, EnumString, Serialize, Deserialize, TS,
)]
#[strum(serialize_all = "lowercase", ascii_case_insensitive)]
#[serde(rename_all = "lowercase")]
#[ts(export, rename_all = "lowercase")]
pub enum OutputFormat {
    Json,
    Markdown,
}

impl OutputFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Markdown => "md",
        }
    }

    /// The name a save dialog shows for the format.
    pub fn label(self) -> &'static str {
        match self {
            Self::Json => "JSON",
            Self::Markdown => "Markdown",
        }
    }
}
