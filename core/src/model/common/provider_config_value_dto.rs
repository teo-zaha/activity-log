use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A provider config value as the settings UI sends it: text for a single value, an array for a
/// list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(untagged)]
#[ts(export, untagged)]
pub enum ProviderConfigValueDto {
    Text(String),
    List(Vec<String>),
}
