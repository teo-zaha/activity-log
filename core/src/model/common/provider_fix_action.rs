use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{local_fix_target::LocalFixTarget, web_fix_action::WebFixAction};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, tag = "kind", rename_all = "camelCase")]
pub enum ProviderFixAction {
    Web { action: WebFixAction },
    Local { target: LocalFixTarget },
}
