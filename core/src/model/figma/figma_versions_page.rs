use serde::Deserialize;

use super::{FigmaVersion, figma_pagination::FigmaPagination};

#[derive(Debug, Deserialize)]
pub(crate) struct FigmaVersionsPage {
    pub(crate) versions: Vec<FigmaVersion>,
    pub(crate) pagination: FigmaPagination,
}
