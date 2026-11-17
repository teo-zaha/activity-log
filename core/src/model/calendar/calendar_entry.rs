use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// One event occurrence from an ICS calendar feed.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct CalendarEntry {
    #[ts(type = "\"calendar\"")]
    pub source: String,
    /// The name of the connection within its kind, such as `oss`; set by the runtime.
    #[serde(default)]
    pub connection: String,
    pub timestamp: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub location: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub organizer: Option<String>,
    pub duration_minutes: i32,
}
