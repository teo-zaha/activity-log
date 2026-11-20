use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::{CalendarTag, ConnectionName, MeetingBody, Timestamp};

/// One event occurrence from an ICS calendar feed.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct CalendarEntry {
    #[ts(type = "\"calendar\"")]
    pub source: CalendarTag,
    /// The connection within its kind, such as `oss`; stamped by the runtime before the entry is
    /// cached or sent.
    #[ts(as = "String")]
    pub connection: ConnectionName,
    /// The source's own id for the entry (a commit hash, an event id), unique within its
    /// connection: `source`, `connection` and `id` identify an entry.
    pub id: String,
    #[ts(as = "String")]
    pub timestamp: Timestamp,
    /// Absent for a meeting without a subject; the UI and the export show a placeholder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub location: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub organizer: Option<String>,
    #[serde(flatten)]
    #[ts(flatten)]
    pub meeting: MeetingBody,
}
