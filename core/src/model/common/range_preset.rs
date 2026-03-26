use serde::{Deserialize, Serialize};
use strum::{AsRefStr, Display, EnumIter, EnumString};
use ts_rs::TS;

#[derive(
    Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq, Display, AsRefStr, EnumString, EnumIter,
)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
#[strum(serialize_all = "camelCase", ascii_case_insensitive)]
pub enum RangePreset {
    Today,
    Yesterday,
    Week,
    #[strum(serialize = "lastweek")]
    LastWeek,
    Month,
    #[strum(serialize = "lastmonth")]
    LastMonth,
}
