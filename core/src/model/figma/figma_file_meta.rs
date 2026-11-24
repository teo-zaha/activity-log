use serde::Deserialize;

use crate::model::Timestamp;

#[derive(Debug, Deserialize)]
pub(crate) struct FigmaFileMeta {
    pub(crate) name: String,
    pub(crate) last_touched_at: Timestamp,
    #[serde(rename = "editorType")]
    pub(crate) editor_type: String,
    pub(crate) url: Option<String>,
}
