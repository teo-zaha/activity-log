use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// One name of a map field and its list, such as a WSL distribution and its folders.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProviderConfigMapEntryDto {
    pub name: String,
    pub items: Vec<String>,
}
