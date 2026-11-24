use serde::Deserialize;

use super::FigmaFileMeta;

#[derive(Debug, Deserialize)]
pub(crate) struct FigmaFileMetaResponse {
    pub(crate) file: FigmaFileMeta,
}
