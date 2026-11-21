use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{change_stack_end::ChangeStackEnd, change_stack_node::ChangeStackNode};

/// One level above a change: the open changes that target its source branch, as far as they
/// were read, and whether that is all of them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct ChangeStackAbove {
    pub changes: Vec<ChangeStackNode>,
    pub end: ChangeStackEnd,
}
