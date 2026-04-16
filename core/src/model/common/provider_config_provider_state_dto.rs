use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::common::{
    provider_config_field_state_dto::ProviderConfigFieldStateDto, source::Source,
};

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct ProviderConfigProviderStateDto {
    pub source: Source,
    pub fields: Vec<ProviderConfigFieldStateDto>,
}
