use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::common::{
    connection_id::ConnectionId, provider_config_field_state_dto::ProviderConfigFieldStateDto,
};

/// One connection's settings as the settings UI edits them.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct ProviderConfigConnectionStateDto {
    pub connection: ConnectionId,
    pub label: String,
    /// Set for a Git connection inside a WSL distribution.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub wsl_distro: Option<String>,
    pub fields: Vec<ProviderConfigFieldStateDto>,
}
