use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;
use ts_rs::TS;

use super::range_preset::RangePreset;

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CollectRequest {
    #[ts(optional)]
    pub since: Option<String>,
    #[ts(optional)]
    pub until: Option<String>,
    #[ts(optional)]
    pub preset: Option<RangePreset>,
}
