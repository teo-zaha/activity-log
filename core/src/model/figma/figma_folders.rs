use serde::Deserialize;

use super::FigmaFolder;

#[derive(Debug, Deserialize)]
pub(crate) struct FigmaFolders {
    pub(crate) folders: Vec<FigmaFolder>,
}
