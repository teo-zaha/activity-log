use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::common::{
    provider_status::ProviderStatus, source::Source, supported_os::SupportedOs,
};

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct ProviderInfo {
    pub id: Source,
    pub status: ProviderStatus,
    pub usable: bool,
    pub supported_os: Vec<SupportedOs>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub reason: Option<String>,
    pub checked_at: String,
}
