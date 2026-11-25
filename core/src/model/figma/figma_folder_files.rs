use serde::Deserialize;

use super::FigmaFolderFile;

#[derive(Debug, Deserialize)]
pub(crate) struct FigmaFolderFiles {
    pub(crate) files: Vec<FigmaFolderFile>,
}
