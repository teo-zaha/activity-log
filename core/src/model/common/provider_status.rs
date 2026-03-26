use serde::{Deserialize, Serialize};
use strum::{AsRefStr, Display, EnumString};
use ts_rs::TS;

#[derive(
    Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq, Display, AsRefStr, EnumString,
)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
#[strum(serialize_all = "lowercase")]
pub enum ProviderStatus {
    Available,
    Unavailable,
    Degraded,
}
