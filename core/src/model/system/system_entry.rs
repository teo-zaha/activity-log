use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::system::system_entry_type::SystemEntryType;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SystemEntry {
    #[ts(type = "\"system\"")]
    pub source: String,
    #[serde(rename = "type")]
    #[ts(rename = "type")]
    pub entry_type: SystemEntryType,
    pub timestamp: String,
    pub label: String,
}
