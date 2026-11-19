use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::common::provider_config_value_type_dto::ProviderConfigValueTypeDto;

/// A field's value and its type, tagged by `shape` so a client can narrow to one of them.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
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

impl std::fmt::Debug for ProviderConfigFieldValueDto {
    // A URL is shown as its endpoint: its user info, query and fragment can hold credentials.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let shown = |value_type: &ProviderConfigValueTypeDto, value: &str| match value_type {
            ProviderConfigValueTypeDto::Url => {
                crate::provider::provider_config::url_for_debug(value)
            }
            _ => value.to_string(),
        };
        match self {
            Self::Single {
                value_type,
                secret,
                value,
            } => f
                .debug_struct("Single")
                .field("value_type", value_type)
                .field("secret", secret)
                .field(
                    "value",
                    &value.as_deref().map(|value| shown(value_type, value)),
                )
                .finish(),
            Self::List { item_type, items } => f
                .debug_struct("List")
                .field("item_type", item_type)
                .field(
                    "items",
                    &items
                        .iter()
                        .map(|item| shown(item_type, item))
                        .collect::<Vec<_>>(),
                )
                .finish(),
        }
    }
}
