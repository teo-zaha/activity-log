use serde::{Deserialize, Serialize};
use strum::{AsRefStr, Display, EnumString};
use ts_rs::TS;

#[derive(
    Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq, Display, AsRefStr, EnumString,
)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
#[strum(serialize_all = "camelCase", ascii_case_insensitive)]
pub enum JiraEntryType {
    IssueCreated,
    IssueUpdated,
    StatusChanged,
}
