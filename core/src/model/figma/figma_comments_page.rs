use serde::Deserialize;

use super::FigmaComment;

#[derive(Debug, Deserialize)]
pub(crate) struct FigmaCommentsPage {
    pub(crate) comments: Vec<FigmaComment>,
}
