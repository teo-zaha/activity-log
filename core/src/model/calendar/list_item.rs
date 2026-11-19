use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::text_segment::TextSegment;

/// An item of a `DescriptionBlock::List`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct ListItem {
    /// 0 for an item of the list itself, 1 for one under such an item, and so on.
    pub depth: u8,
    pub segments: Vec<TextSegment>,
}
