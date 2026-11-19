use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::common::{
    connection_id::ConnectionId, provider_config_save_response_dto::ProviderConfigSaveResponseDto,
};

/// The settings after adding a connection, and the id the new connection got.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct AddConnectionResponseDto {
    pub created: ConnectionId,
    #[serde(flatten)]
    #[ts(flatten)]
    pub saved: ProviderConfigSaveResponseDto,
}
