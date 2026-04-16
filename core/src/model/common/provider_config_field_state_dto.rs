use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::common::provider_config_value_type_dto::ProviderConfigValueTypeDto;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct ProviderConfigFieldStateDto {
    pub key: String,
    pub label: String,
    pub required: bool,
    pub value_type: ProviderConfigValueTypeDto,
    pub secret: bool,
    pub is_set: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub value: Option<String>,
}
