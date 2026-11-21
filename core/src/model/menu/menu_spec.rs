use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::menu::menu_section::MenuSection;

/// The dashboard's part of the macOS menu bar, built from its shortcut table: the desktop app
/// puts the system's own items around it (`install_menu`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct MenuSpec {
    pub sections: Vec<MenuSection>,
}
