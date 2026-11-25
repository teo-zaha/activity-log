use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::calendar_feed_service::CalendarFeedService;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CalendarFeedGuide {
    pub service: CalendarFeedService,
    pub label: String,
    pub instructions: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub url: Option<String>,
}
