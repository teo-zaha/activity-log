use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// What became of an export the desktop app saves through its dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub enum ExportOutcome {
    Saved,
    /// The user closed the dialog; nothing was written.
    Cancelled,
}
