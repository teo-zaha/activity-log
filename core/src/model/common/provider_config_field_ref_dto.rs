use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Which field of a connection's settings a field issue is about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, tag = "kind", rename_all = "camelCase")]
pub enum ProviderConfigFieldRefDto {
    Label,
    /// A setting by its wire name, `GITLAB_URL`.
    Value {
        key: String,
    },
    /// One item of a list setting, by its place among the items sent, blank ones not counted.
    Item {
        key: String,
        index: u32,
    },
    /// The connection as a whole: it was removed or replaced, or its base is unknown.
    Connection,
}
