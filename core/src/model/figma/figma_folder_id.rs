use serde::{Deserialize, Serialize};

use crate::provider::remote_api::is_path_segment;

/// Figma's opaque identifier for a folder.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct FigmaFolderId(#[serde(deserialize_with = "deserialize_id")] String);

impl FigmaFolderId {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

fn deserialize_id<'de, D: serde::Deserializer<'de>>(de: D) -> Result<String, D::Error> {
    let id = String::deserialize(de)?;
    if !is_path_segment(&id) {
        return Err(serde::de::Error::custom("invalid Figma folder ID"));
    }
    Ok(id)
}
