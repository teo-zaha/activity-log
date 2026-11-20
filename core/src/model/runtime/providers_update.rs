use std::sync::Arc;

use serde::Serialize;
use ts_rs::TS;

use crate::model::common::providers_response::ProvidersResponse;

/// The providers part of a host's state with its stamp (`docs/plans/shared-contracts.md` §5).
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct ProvidersUpdate {
    pub epoch: String,
    pub authority: String,
    #[ts(type = "number")]
    pub revision: u64,
    #[ts(as = "ProvidersResponse")]
    pub providers: Arc<ProvidersResponse>,
}
