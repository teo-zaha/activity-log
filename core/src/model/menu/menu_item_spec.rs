use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::menu::{
    menu_item_key::MenuItemKey, menu_item_state::MenuItemState, menu_key::MenuKey,
};

/// One of the dashboard's menu items: its key, its accelerator if it has one, and what it shows
/// at first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct MenuItemSpec {
    pub key: MenuItemKey,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accelerator: Option<MenuKey>,
    pub state: MenuItemState,
}
