use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Where one part of a host's state stands: the host (`epoch`), the authority it was decided
/// under and the revision at which the part last changed. The two strings are opaque and are
/// compared only for equality; revisions only grow within an epoch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct PartStamp {
    pub epoch: String,
    pub authority: String,
    #[ts(type = "number")]
    pub revision: u64,
}
