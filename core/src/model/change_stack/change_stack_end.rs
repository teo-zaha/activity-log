use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::stack_cut::StackCut;

/// How one level above a change ends: every open change on it was read, or the walk stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, tag = "kind", rename_all = "camelCase")]
pub enum ChangeStackEnd {
    Complete,
    Cut { reason: StackCut },
}
