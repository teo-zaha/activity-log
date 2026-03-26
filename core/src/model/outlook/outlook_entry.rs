use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;
use ts_rs::TS;

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct OutlookEntry {
    #[ts(type = "\"outlook\"")]
    pub source: String,
    pub timestamp: String,
    pub title: String,
    #[ts(optional)]
    pub location: Option<String>,
    #[ts(optional)]
    pub organizer: Option<String>,
    pub is_organizer: bool,
    pub duration_minutes: i32,
}
