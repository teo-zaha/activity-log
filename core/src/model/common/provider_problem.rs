use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Why a connection's startup check did not pass cleanly; its `reason` says it in words.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub enum ProviderProblem {
    /// The check did not answer in time.
    TimedOut,
    /// The provider does not run on this OS.
    Unsupported,
    /// A required setting is missing or a value does not fit its setting.
    NotConfigured,
    /// The check ran and reported a problem.
    Failed,
}
