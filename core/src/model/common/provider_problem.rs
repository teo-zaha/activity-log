use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Why a connection's check or last collection did not pass cleanly; its `reason` says it in
/// words.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq, strum::EnumIter)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub enum ProviderProblem {
    /// The check or the collection did not answer in time.
    TimedOut,
    /// The provider does not run on this OS.
    Unsupported,
    /// A required setting is missing or a value does not fit its setting.
    NotConfigured,
    /// The server refused the credentials with 401 or a 403 without signs of throttling.
    Refused,
    /// The server identified a permission missing from the token.
    ScopeMissing,
    /// The network calendar feed could not be accessed at its configured link.
    FeedUnavailable,
    /// The server is busy. Check again later.
    Throttled,
    /// The server could not be reached.
    Unreachable,
    /// The app the source reads is not installed.
    NotInstalled,
    /// The app the source reads is not running.
    NotRunning,
    /// The OS refused access (macOS Automation, an elevated app, the event log).
    PermissionDenied,
    /// There is nothing to read, such as no Git repository in the folders searched.
    NoData,
    /// The check ran and reported another problem.
    Failed,
    /// The check passed, but the last collection failed for another reason.
    CollectionFailed,
    /// The secure store that holds the key to the connection's secrets gave no key: missing,
    /// refusing (a dismissed dialog, no interaction allowed) or failing. Check again once it
    /// answers.
    SecretUnavailable,
}
