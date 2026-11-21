use serde::{Deserialize, Serialize};
use strum::EnumIter;
use ts_rs::TS;

/// A submenu of the macOS menu bar, in the bar's order. The dashboard's shortcut table names
/// the one each of its entries is listed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EnumIter, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub enum MenuSubmenu {
    App,
    File,
    Edit,
    View,
    Window,
    Help,
}
