use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ActivityLogMeta {
    pub generated: String,
    pub since: String,
    pub until: String,
}
