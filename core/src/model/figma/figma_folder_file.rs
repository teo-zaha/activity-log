use serde::Deserialize;
use serde_json::Value;

use crate::model::Timestamp;

#[derive(Debug, Deserialize)]
pub(crate) struct FigmaFolderFile {
    #[serde(default)]
    pub(crate) key: Value,
    pub(crate) last_modified: Timestamp,
}
