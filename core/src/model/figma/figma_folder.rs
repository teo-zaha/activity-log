use serde::Deserialize;

use super::FigmaFolderId;

#[derive(Debug, Deserialize)]
pub(crate) struct FigmaFolder {
    pub(crate) id: FigmaFolderId,
    // The field is required and null for a top-level folder.
    #[serde(deserialize_with = "Option::deserialize")]
    pub(crate) parent_folder_id: Option<FigmaFolderId>,
}
