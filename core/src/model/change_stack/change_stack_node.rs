use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{change_stack_above::ChangeStackAbove, stacked_change::StackedChange};

/// A change above another, with what lies above it in turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct ChangeStackNode {
    pub change: StackedChange,
    pub above: ChangeStackAbove,
}
