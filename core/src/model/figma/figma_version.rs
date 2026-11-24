use serde::{Deserialize, Deserializer, de::Error};

use super::figma_user::FigmaUser;
use crate::model::Timestamp;

#[derive(Debug, Deserialize)]
pub(crate) struct FigmaVersion {
    #[serde(deserialize_with = "numeric_id")]
    pub(crate) id: String,
    pub(crate) created_at: Timestamp,
    pub(crate) user: FigmaUser,
    #[serde(deserialize_with = "Option::deserialize")]
    pub(crate) label: Option<String>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub(crate) description: Option<String>,
}

fn numeric_id<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    let id = String::deserialize(deserializer)?;
    // A version id becomes the numeric `before` query value. Keep its original spelling.
    if !id.bytes().all(|byte| byte.is_ascii_digit()) || id.parse::<u64>().is_err() {
        return Err(D::Error::custom("expected a numeric Figma version id"));
    }
    Ok(id)
}
