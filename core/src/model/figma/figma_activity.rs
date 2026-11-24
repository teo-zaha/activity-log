use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(
    export,
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum FigmaActivity {
    VersionSaved {
        version_id: String,
        label: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        description: Option<String>,
    },
    Edited {
        version_id: String,
    },
    Commented {
        comment_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        message: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        node_id: Option<String>,
        resolved: bool,
    },
    Replied {
        comment_id: String,
        parent_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        message: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        node_id: Option<String>,
    },
}
