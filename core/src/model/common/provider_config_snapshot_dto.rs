use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::common::{
    provider_config_connection_state_dto::ProviderConfigConnectionStateDto,
    provider_kind_dto::ProviderKindDto,
};

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct ProviderConfigSnapshotDto {
    /// Names the file as this snapshot read it; a save or removal sends it back as `basedOn`,
    /// so a change another program made meanwhile is a conflict, not overwritten.
    pub revision: String,
    pub kinds: Vec<ProviderKindDto>,
    pub connections: Vec<ProviderConfigConnectionStateDto>,
}
