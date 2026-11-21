use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// What a menu item shows: a command, or one that carries a check mark (a view, checked while it
/// is on screen). An item keeps its kind for as long as its menu is installed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, tag = "kind", rename_all = "camelCase")]
pub enum MenuItemState {
    Command {
        title: String,
        enabled: bool,
    },
    Check {
        title: String,
        enabled: bool,
        checked: bool,
    },
}

impl MenuItemState {
    pub fn title(&self) -> &str {
        match self {
            Self::Command { title, .. } | Self::Check { title, .. } => title,
        }
    }

    pub fn enabled(&self) -> bool {
        match self {
            Self::Command { enabled, .. } | Self::Check { enabled, .. } => *enabled,
        }
    }

    /// Whether `other` is a state of the same kind of item.
    pub fn same_kind(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (Self::Command { .. }, Self::Command { .. }) | (Self::Check { .. }, Self::Check { .. })
        )
    }
}
