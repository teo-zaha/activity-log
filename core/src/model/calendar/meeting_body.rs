use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{
    busy_status::BusyStatus, description_block::DescriptionBlock, meeting_weight::MeetingWeight,
};

/// What every meeting has, whichever source it comes from: how long it is, how it shows in
/// free/busy time and its description. Flattened into the meeting entries.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct MeetingBody {
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
