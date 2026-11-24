use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::ChangeStack;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct PullRequestRef {
    #[ts(type = "number")]
    pub number: u64,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub web_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub change_stack: Option<ChangeStack>,
}
