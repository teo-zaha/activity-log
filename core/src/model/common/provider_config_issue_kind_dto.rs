use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// What is wrong with a field of a settings save.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub enum ProviderConfigIssueKindDto {
    /// The value does not fit its setting.
    Invalid,
    /// Another program changed this field since the draft's settings were read.
    Changed,
    /// Another program removed the connection.
    Removed,
    /// Another program removed the connection and added one of the same name.
    Replaced,
    /// The settings the draft was built from are no longer known: read them again.
    UnknownBase,
}
