use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Which of Activity Log 0.0.7's settings files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub enum LegacySource {
    /// The desktop app's `settings.json`.
    Settings,
    /// The CLI's and the API server's `.env`.
    Env,
}
