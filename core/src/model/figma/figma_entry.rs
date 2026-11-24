use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{FigmaActivity, FigmaEditor};
use crate::model::{ConnectionName, FigmaTag, Timestamp};

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct FigmaEntry {
    #[ts(type = "\"figma\"")]
    pub source: FigmaTag,
    #[ts(as = "String")]
    pub connection: ConnectionName,
    pub id: String,
    #[ts(as = "String")]
    pub timestamp: Timestamp,
    pub file_key: String,
    pub file_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub editor: Option<FigmaEditor>,
    // The file or its comment's frame, when Figma supplied a node id.
    pub file_url: String,
    #[serde(flatten)]
    #[ts(flatten)]
    pub activity: FigmaActivity,
}
