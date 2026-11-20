use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Where a settings change became visible: the providers part's revision, in its host's epoch,
/// that first holds it, or the current one for a change that changed nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct InstallReceipt {
    pub epoch: String,
    #[ts(type = "number")]
    pub revision: u64,
}
