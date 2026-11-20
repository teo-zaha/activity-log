use serde::Serialize;
use ts_rs::TS;

use crate::model::{common::providers_response::ProvidersResponse, runtime::part_stamp::PartStamp};

/// What `POST /api/connections/recheck` and the desktop's `recheck_connection` answer: the
/// providers as installed, the connection checking, and that part's stamp.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct RecheckAnswer {
    #[serde(flatten)]
    #[ts(flatten)]
    pub providers: ProvidersResponse,
    pub stamp: PartStamp,
}
