use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::common::provider_config_value_type_dto::ProviderConfigValueTypeDto;

/// A field's value and its type, tagged by `shape` so a client can narrow to one or the other.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "shape", rename_all = "camelCase")]
#[ts(export, tag = "shape", rename_all = "camelCase")]
pub enum ProviderConfigFieldValueDto {
    #[serde(rename_all = "camelCase")]
    #[ts(rename_all = "camelCase")]
    Single {
        value_type: ProviderConfigValueTypeDto,
        /// Secrets are write-only: `value` is never sent for them.
        secret: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        value: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    #[ts(rename_all = "camelCase")]
    List {
        item_type: ProviderConfigValueTypeDto,
        /// The configured items, or the default the provider uses while the key is unset.
        items: Vec<String>,
    },
}
