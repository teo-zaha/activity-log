use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::common::source::Source;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct AddConnectionRequestDto {
    pub source: Source,
    /// Defaults to the kind's name, or the location for Git.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub label: Option<String>,
    /// For Git: read this WSL distribution instead of this computer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub wsl_distro: Option<String>,
}
