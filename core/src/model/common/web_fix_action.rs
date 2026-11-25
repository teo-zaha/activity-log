use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::calendar_feed_guide::CalendarFeedGuide;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, tag = "kind", rename_all = "camelCase")]
pub enum WebFixAction {
    CreateToken {
        url: String,
        instructions: String,
        #[serde(rename = "classicUrl", skip_serializing_if = "Option::is_none")]
        #[ts(optional, rename = "classicUrl")]
        classic_url: Option<String>,
    },
    FeedHelp {
        guides: Vec<CalendarFeedGuide>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        recovery: Option<String>,
    },
}
