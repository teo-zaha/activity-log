use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::common::provider_config_value_dto::ProviderConfigValueDto;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct ProviderConfigSaveValueDto {
    pub key: String,
    /// `None` clears the key, so its default applies again.
    pub value: Option<ProviderConfigValueDto>,
}
