use serde::Deserialize;

use super::{figma_comment_position::FigmaCommentPosition, figma_user::FigmaUser};
use crate::model::Timestamp;

#[derive(Debug, Deserialize)]
pub(crate) struct FigmaComment {
    pub(crate) id: String,
    pub(crate) user: FigmaUser,
    pub(crate) created_at: Timestamp,
    pub(crate) parent_id: Option<String>,
    pub(crate) message: String,
    pub(crate) resolved_at: Option<Timestamp>,
    pub(crate) client_meta: Option<FigmaCommentPosition>,
}

impl FigmaComment {
    pub(crate) fn node_id(&self) -> Option<&str> {
        self.client_meta.as_ref()?.node_id.as_deref()
    }
}
