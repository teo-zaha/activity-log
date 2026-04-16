use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::common::provider_config_provider_state_dto::ProviderConfigProviderStateDto;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct ProviderConfigSnapshotDto {
    pub providers: Vec<ProviderConfigProviderStateDto>,
}
