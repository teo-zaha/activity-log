use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// One open change of a stack, as the host had it when the collection ran.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct StackedChange {
    /// The host's number for it in its repository (GitLab `iid`, GitHub `number`).
    #[ts(type = "number")]
    pub number: u64,
    pub title: String,
    /// Its page: http(s), on the host's web origin, without user info.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub web_url: Option<String>,
}
