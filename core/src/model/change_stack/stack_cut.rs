use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Why a change stack ends before its end was read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub enum StackCut {
    /// A depth, member, request or time limit stopped the walk here.
    Limit,
    /// The host refused or hid what lies beyond (the token's scope, a hidden repository).
    Unreadable,
    /// The host asked to slow down; nothing more was asked.
    Throttled,
    /// The request failed otherwise.
    Failed,
    /// The branches lead back to a change already on the path.
    Cycle,
}
