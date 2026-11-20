use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::{
    common::provider_config_snapshot_dto::ProviderConfigSnapshotDto,
    common::providers_response::ProvidersResponse, runtime::install_receipt::InstallReceipt,
};

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct ProviderConfigSaveResponseDto {
    pub snapshot: ProviderConfigSnapshotDto,
    pub providers_response: ProvidersResponse,
    /// Where the change became visible, see [`InstallReceipt`].
    pub installed: InstallReceipt,
}
