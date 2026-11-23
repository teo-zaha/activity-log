use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::common::{
    connection_id::ConnectionId, provider_config_save_value_dto::ProviderConfigSaveValueDto,
};

/// Changes to one connection: a new label and changed values.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct ProviderConfigSaveRequestDto {
    pub connection: ConnectionId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub label: Option<String>,
    pub values: Vec<ProviderConfigSaveValueDto>,
    /// The snapshot `revision` the draft was built from: a save of fields another program
    /// changed since is a conflict.
    pub based_on: String,
}
