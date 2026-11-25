//! What kind of problem a check's or a collection's failure is, told from the typed causes in
//! its `anyhow` chain: the server's answer (`RemoteFailure`), a local source's state
//! (`SourceFailure`), an anonymous Jira response, or an AppleScript error code. Messages are
//! for people; nothing here reads them.

use std::fmt;

use crate::{
    model::{LocalApp, ProviderProblem},
    provider::{jira::api::JiraResponseError, remote_error::RemoteFailure},
};

/// Typed evidence used to select recovery for a settled health outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixCause {
    AutomationDenied,
    FolderDenied,
    AppNotRunning(LocalApp),
}

impl FixCause {
    pub fn problem(self) -> ProviderProblem {
        match self {
            Self::AutomationDenied | Self::FolderDenied => ProviderProblem::PermissionDenied,
            Self::AppNotRunning(_) => ProviderProblem::NotRunning,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClassifiedProblem(Classification);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Classification {
    Plain(ProviderProblem),
    Caused(FixCause),
}

impl ClassifiedProblem {
    pub fn plain(problem: ProviderProblem) -> Self {
        Self(Classification::Plain(problem))
    }

    pub fn caused(cause: FixCause) -> Self {
        Self(Classification::Caused(cause))
    }

    pub fn problem(self) -> ProviderProblem {
        match self.0 {
            Classification::Plain(problem) => problem,
            Classification::Caused(cause) => cause.problem(),
        }
    }

    pub fn cause(self) -> Option<FixCause> {
        match self.0 {
            Classification::Plain(_) => None,
            Classification::Caused(cause) => Some(cause),
        }
    }
}

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
    classification: ClassifiedProblem,
    pub message: String,
}

impl SourceFailure {
    pub fn new(problem: SourceProblem, message: impl Into<String>) -> Self {
        Self {
            classification: ClassifiedProblem::plain(problem.into()),
            message: message.into(),
        }
    }

    pub fn caused(cause: FixCause, message: impl Into<String>) -> Self {
        Self {
            classification: ClassifiedProblem::caused(cause),
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
pub fn problem_of(err: &anyhow::Error) -> Option<ClassifiedProblem> {
    err.chain().find_map(|cause| {
        if let Some(failure) = cause.downcast_ref::<RemoteFailure>() {
            return failure.error.problem().map(ClassifiedProblem::plain);
        }
        if matches!(
            cause.downcast_ref::<JiraResponseError>(),
            Some(JiraResponseError::Anonymous)
        ) {
            return Some(ClassifiedProblem::plain(ProviderProblem::Refused));
        }
        if let Some(failure) = cause.downcast_ref::<SourceFailure>() {
            return Some(failure.classification);
        }
        // The key store gave no key, whatever it took: never a slow server or a timeout.
        if cause.is::<crate::secrets::StoreError>() {
            return Some(ClassifiedProblem::plain(ProviderProblem::SecretUnavailable));
        }
        apple_script_problem(cause)
    })
}

#[cfg(target_os = "macos")]
fn apple_script_problem(cause: &(dyn std::error::Error + 'static)) -> Option<ClassifiedProblem> {
    if cause.is::<crate::collect::outlook::OutlookNotRunning>() {
        return Some(ClassifiedProblem::caused(FixCause::AppNotRunning(
            LocalApp::Outlook,
        )));
    }
    let problem = cause
        .downcast_ref::<crate::platform::macos::AppleScriptError>()?
        .problem()?;
    Some(match problem {
        SourceProblem::PermissionDenied => ClassifiedProblem::caused(FixCause::AutomationDenied),
        _ => ClassifiedProblem::plain(problem.into()),
    })
}

#[cfg(not(target_os = "macos"))]
fn apple_script_problem(_cause: &(dyn std::error::Error + 'static)) -> Option<ClassifiedProblem> {
    None
}

#[cfg(test)]
mod tests;
