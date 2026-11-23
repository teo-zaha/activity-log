use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::common::legacy_source::LegacySource;

/// One of 0.0.7's settings files where it was found, its path for people.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct LegacyFileDto {
    pub source: LegacySource,
    pub path: String,
}
