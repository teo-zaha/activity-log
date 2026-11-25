use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::common::{
    connection_id::ConnectionId, provider_fix_action::ProviderFixAction,
    provider_problem::ProviderProblem, provider_status::ProviderStatus, supported_os::SupportedOs,
};

/// One connection and the result of its startup check.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct ProviderInfo {
    pub connection: ConnectionId,
    pub label: String,
    /// Set for a Git connection inside a WSL distribution.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub wsl_distro: Option<String>,
    pub status: ProviderStatus,
    pub usable: bool,
    pub supported_os: Vec<SupportedOs>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub reason: Option<String>,
    /// Set with `reason`: what kind of problem it describes.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub problem: Option<ProviderProblem>,
    /// Whether the UI offers to check it again: it has a problem other than `unsupported`.
    pub recheckable: bool,
    pub checked_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub fix_action: Option<ProviderFixAction>,
}
