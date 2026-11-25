use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{
    provider_config_field_value_dto::ProviderConfigFieldValueDto, web_fix_action::WebFixAction,
};

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct ProviderConfigFieldStateDto {
    pub key: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub placeholder: Option<String>,
    pub required: bool,
    pub is_set: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub fix_action: Option<WebFixAction>,
    #[serde(flatten)]
    #[ts(flatten)]
    pub value: ProviderConfigFieldValueDto,
}
