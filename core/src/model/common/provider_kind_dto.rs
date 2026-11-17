use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::common::{source::Source, supported_os::SupportedOs};

/// A provider kind, and what can be added for it in the settings.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct ProviderKindDto {
    pub source: Source,
    pub label: String,
    /// Whether more than one connection of the kind can be added.
    pub multiple: bool,
    pub supported_os: Vec<SupportedOs>,
    /// Whether the kind runs on this OS.
    pub supported: bool,
    /// For Git on Windows: the WSL distributions a connection can read.
    pub wsl_distros: Vec<String>,
}
