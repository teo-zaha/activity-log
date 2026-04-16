use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::common::{
    provider_config_save_value_dto::ProviderConfigSaveValueDto, source::Source,
};

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct ProviderConfigSaveRequestDto {
    pub source: Source,
    pub values: Vec<ProviderConfigSaveValueDto>,
}
