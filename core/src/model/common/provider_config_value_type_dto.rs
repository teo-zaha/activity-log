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
