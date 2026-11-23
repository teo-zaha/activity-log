//! What kind of problem a check's or a collection's failure is, told from the typed causes in
//! its `anyhow` chain: the server's answer (`RemoteFailure`), a local source's state
//! (`SourceFailure`), an AppleScript error code. Messages are for people; nothing here reads
//! them.

use std::fmt;

use crate::{model::ProviderProblem, provider::remote_error::RemoteFailure};

/// A local source that cannot be read, and why, in a form the UI can act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceProblem {
    /// The app the source reads is not installed (Outlook, or only the new Outlook, which has
    /// no interface to read).
    NotInstalled,
    /// The app is installed but not running.
    NotRunning,
    /// The OS refused access: macOS Automation, an elevated app, the event log.
    PermissionDenied,
    /// There is nothing to read, such as no Git repository in the folders searched.
    NoData,
}

impl From<SourceProblem> for ProviderProblem {
    fn from(problem: SourceProblem) -> Self {
        match problem {
            SourceProblem::NotInstalled => Self::NotInstalled,
            SourceProblem::NotRunning => Self::NotRunning,
            SourceProblem::PermissionDenied => Self::PermissionDenied,
            SourceProblem::NoData => Self::NoData,
        }
    }
}

/// A local source's failure: its problem, and what to tell people.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFailure {
    pub problem: SourceProblem,
    pub message: String,
}

impl SourceFailure {
    pub fn new(problem: SourceProblem, message: impl Into<String>) -> Self {
        Self {
            problem,
            message: message.into(),
        }
    }
}

impl fmt::Display for SourceFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for SourceFailure {}

/// The problem the first typed cause in `err`'s chain names; `None` when no cause says more
/// than that something failed.
pub fn problem_of(err: &anyhow::Error) -> Option<ProviderProblem> {
    err.chain().find_map(|cause| {
        if let Some(failure) = cause.downcast_ref::<RemoteFailure>() {
            return failure.error.problem();
        }
        if let Some(failure) = cause.downcast_ref::<SourceFailure>() {
            return Some(failure.problem.into());
        }
        // The key store gave no key, whatever it took: never a slow server or a timeout.
        if cause.is::<crate::secrets::StoreError>() {
            return Some(ProviderProblem::SecretUnavailable);
        }
        apple_script_problem(cause)
    })
}

#[cfg(target_os = "macos")]
fn apple_script_problem(cause: &(dyn std::error::Error + 'static)) -> Option<ProviderProblem> {
    cause
        .downcast_ref::<crate::platform::macos::AppleScriptError>()
        .and_then(|err| err.problem())
        .map(ProviderProblem::from)
}

#[cfg(not(target_os = "macos"))]
fn apple_script_problem(_cause: &(dyn std::error::Error + 'static)) -> Option<ProviderProblem> {
    None
}

#[cfg(test)]
mod tests;
