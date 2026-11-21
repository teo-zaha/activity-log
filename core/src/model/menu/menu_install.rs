use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// What `install_menu` did: the menu is the app's menu from now on, under `generation`, which
/// its clicks carry and its updates name; or this system has no app menu (Windows, Linux).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, tag = "kind", rename_all = "camelCase")]
pub enum MenuInstall {
    Installed { generation: u32 },
    NotOffered,
}
