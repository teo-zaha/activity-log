use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::range_preset::RangePreset;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CollectRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub since: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub until: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub preset: Option<RangePreset>,
}
