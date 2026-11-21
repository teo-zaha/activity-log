use serde::{Deserialize, Serialize};
use strum::EnumIter;
use ts_rs::TS;

/// The key of a menu item's accelerator, which is always ⌘ (Ctrl elsewhere) and this key: the
/// keys the dashboard's shortcut table gives the menu, and no others.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EnumIter, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub enum MenuKey {
    R,
    F,
    BracketLeft,
    Comma,
    Digit1,
    Digit2,
    Digit3,
    Digit4,
}

impl MenuKey {
    /// The accelerator as Tauri's menu reads it.
    pub fn accelerator(self) -> &'static str {
        match self {
            Self::R => "CmdOrCtrl+R",
            Self::F => "CmdOrCtrl+F",
            Self::BracketLeft => "CmdOrCtrl+[",
            Self::Comma => "CmdOrCtrl+,",
            Self::Digit1 => "CmdOrCtrl+1",
            Self::Digit2 => "CmdOrCtrl+2",
            Self::Digit3 => "CmdOrCtrl+3",
            Self::Digit4 => "CmdOrCtrl+4",
        }
    }
}
