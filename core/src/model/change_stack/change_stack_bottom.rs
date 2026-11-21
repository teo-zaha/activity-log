use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::stack_cut::StackCut;

/// How the way down from a change ends.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, tag = "kind", rename_all = "camelCase")]
pub enum ChangeStackBottom {
    /// The lowest change targets a branch no open change comes from, such as `main`.
    Reached {
        branch: String,
    },
    /// Several open changes come from the branch the lowest one targets.
    Ambiguous,
    Cut {
        reason: StackCut,
    },
}
