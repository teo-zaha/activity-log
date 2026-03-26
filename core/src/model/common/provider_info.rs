use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;
use ts_rs::TS;

use crate::model::common::{
    provider_status::ProviderStatus, source::Source, supported_os::SupportedOs,
};

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct ProviderInfo {
    pub id: Source,
    pub status: ProviderStatus,
    pub usable: bool,
    pub supported_os: Vec<SupportedOs>,
    #[ts(optional)]
    pub reason: Option<String>,
    pub checked_at: String,
}
