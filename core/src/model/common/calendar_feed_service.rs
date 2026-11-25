use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub enum CalendarFeedService {
    OutlookWork,
    OutlookCom,
    GoogleCalendar,
    #[serde(rename = "iCloud")]
    #[ts(rename = "iCloud")]
    ICloud,
    Fastmail,
}
