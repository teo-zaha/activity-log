use crate::provider::provider_config::ProviderConfigValueType;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[ts(export, rename_all = "lowercase")]
pub enum ProviderConfigValueTypeDto {
    String,
    Bool,
    Int,
    Password,
    Email,
    Url,
}

impl From<ProviderConfigValueType> for ProviderConfigValueTypeDto {
    fn from(value_type: ProviderConfigValueType) -> Self {
        match value_type {
            ProviderConfigValueType::String => Self::String,
            ProviderConfigValueType::Bool => Self::Bool,
            ProviderConfigValueType::Int => Self::Int,
            ProviderConfigValueType::Password => Self::Password,
            ProviderConfigValueType::Email => Self::Email,
            ProviderConfigValueType::Url => Self::Url,
        }
    }
}
