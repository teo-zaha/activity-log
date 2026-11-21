use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::menu::menu_item_key::MenuItemKey;

/// The desktop's `menu-command` event: the item of the menu installed under `generation` that
/// was chosen, by click or by its accelerator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct MenuCommand {
    pub generation: u32,
    pub key: MenuItemKey,
}
