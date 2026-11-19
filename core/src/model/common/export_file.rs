use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A rendered export: the file name to suggest and the file's text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct ExportFile {
    pub file_name: String,
    pub content: String,
}
