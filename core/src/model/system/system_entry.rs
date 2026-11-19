use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::system::system_entry_type::SystemEntryType;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SystemEntry {
    #[ts(type = "\"system\"")]
    pub source: String,
    /// The name of the connection within its kind, such as `oss`; set by the runtime.
    #[serde(default)]
    pub connection: String,
    /// The source's own id for the entry (a commit hash, an event id), unique within its
    /// connection: `source`, `connection` and `id` identify an entry.
    pub id: String,
    #[serde(rename = "type")]
    #[ts(rename = "type")]
    pub entry_type: SystemEntryType,
    pub timestamp: String,
    pub label: String,
}
