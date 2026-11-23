use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::common::{
    provider_info::ProviderInfo, secret_store_info::SecretStoreInfo, supported_os::SupportedOs,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct ProvidersResponse {
    pub current_os: SupportedOs,
    pub providers: Vec<ProviderInfo>,
    /// Where new secrets are sealed here.
    pub secret_store: SecretStoreInfo,
}
