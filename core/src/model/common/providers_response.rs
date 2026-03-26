use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::common::{provider_info::ProviderInfo, supported_os::SupportedOs};

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct ProvidersResponse {
    pub current_os: SupportedOs,
    pub providers: Vec<ProviderInfo>,
}
