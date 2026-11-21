use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::menu::{menu_item_key::MenuItemKey, menu_item_state::MenuItemState};

/// What an installed menu item shows from now on (`update_menu`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct MenuItemUpdate {
    pub key: MenuItemKey,
    pub state: MenuItemState,
}
