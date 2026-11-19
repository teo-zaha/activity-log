use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::{BusyStatus, DescriptionBlock, MeetingWeight};

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct OutlookEntry {
    #[ts(type = "\"outlook\"")]
    pub source: String,
    /// The name of the connection within its kind, such as `oss`; set by the runtime.
    #[serde(default)]
    pub connection: String,
    /// The source's own id for the entry (a commit hash, an event id), unique within its
    /// connection: `source`, `connection` and `id` identify an entry.
    pub id: String,
    pub timestamp: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub location: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub organizer: Option<String>,
    pub is_organizer: bool,
    pub duration_minutes: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub busy_status: Option<BusyStatus>,
    /// How the entry counts towards meeting time, from `busy_status`.
    pub meeting_weight: MeetingWeight,
    /// The description as plain text, without the online-meeting invitation and capped in
    /// length.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub description: Option<String>,
    /// `description` as paragraphs, headings and lists of text and links, for rendering.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(as = "Option<Vec<DescriptionBlock>>", optional)]
    pub description_blocks: Vec<DescriptionBlock>,
    /// The https link that joins the online meeting (Teams, Zoom, Google Meet, Webex).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub join_url: Option<String>,
}
