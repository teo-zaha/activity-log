use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A provider config value as the settings UI sends it: text for a single value, an array for a
/// list.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(untagged)]
#[ts(export, untagged)]
pub enum ProviderConfigValueDto {
    Text(String),
    List(Vec<String>),
}

impl std::fmt::Debug for ProviderConfigValueDto {
    // The value can be a token: only its shape.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Text(text) => write!(f, "Text(<{} chars>)", text.chars().count()),
            Self::List(items) => write!(f, "List(<{} items>)", items.len()),
        }
    }
}
