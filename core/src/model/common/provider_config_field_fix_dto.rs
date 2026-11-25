use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::web_fix_action::WebFixAction;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProviderConfigFieldFixDto {
    pub key: String,
    pub action: WebFixAction,
}
