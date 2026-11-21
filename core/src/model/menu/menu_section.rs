use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::menu::{menu_item_spec::MenuItemSpec, menu_submenu::MenuSubmenu};

/// Items that belong together in one submenu; a separator stands between two sections of one
/// submenu and between the app's items and the system's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct MenuSection {
    pub submenu: MenuSubmenu,
    pub items: Vec<MenuItemSpec>,
}
