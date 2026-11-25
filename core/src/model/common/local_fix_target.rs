use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{local_app::LocalApp, system_settings_pane::SystemSettingsPane};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, tag = "kind", rename_all = "camelCase")]
pub enum LocalFixTarget {
    SystemSettings { pane: SystemSettingsPane },
    OpenApp { app: LocalApp },
}
