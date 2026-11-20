use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::{
    ConnectionName, SystemTag, Timestamp, system::system_entry_type::SystemEntryType,
};

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SystemEntry {
    #[ts(type = "\"system\"")]
    pub source: SystemTag,
    /// The connection within its kind, such as `oss`; stamped by the runtime before the entry is
    /// cached or sent.
    #[ts(as = "String")]
    pub connection: ConnectionName,
    /// The source's own id for the entry (a commit hash, an event id), unique within its
    /// connection: `source`, `connection` and `id` identify an entry.
    pub id: String,
    #[serde(rename = "type")]
    #[ts(rename = "type")]
    pub entry_type: SystemEntryType,
    #[ts(as = "String")]
    pub timestamp: Timestamp,
    pub label: String,
}
