use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub enum RangePreset {
    Today,
    Yesterday,
    Week,
    LastWeek,
    Month,
    LastMonth,
}

impl std::str::FromStr for RangePreset {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "today" => Ok(Self::Today),
            "yesterday" => Ok(Self::Yesterday),
            "week" => Ok(Self::Week),
            "lastweek" => Ok(Self::LastWeek),
            "month" => Ok(Self::Month),
            "lastmonth" => Ok(Self::LastMonth),
            _ => Err(format!(
                "Unknown range: \"{value}\". Use today|yesterday|week|lastweek|month|lastmonth"
            )),
        }
    }
}
