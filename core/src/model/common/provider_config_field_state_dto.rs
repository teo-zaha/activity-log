use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::common::provider_config_field_value_dto::ProviderConfigFieldValueDto;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct ProviderConfigFieldStateDto {
    pub key: String,
    pub label: String,
    pub required: bool,
    pub is_set: bool,
    #[serde(flatten)]
    #[ts(flatten)]
    pub value: ProviderConfigFieldValueDto,
}
