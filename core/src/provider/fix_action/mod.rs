//! Selects an action for the outcome the host is about to publish.

pub(crate) mod credential;
pub use credential::CredentialSetup;

use crate::model::{
    LocalApp, LocalFixTarget, ProviderFixAction, ProviderInfo, ProviderProblem, ProviderStatus,
    Source, SupportedOs, SystemSettingsPane,
};

use super::{
    connection::{Connection, ConnectionLocation},
    source_failure::FixCause,
};

/// The front end that owns the host and receives its provider responses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixSurface {
    Web,
    Desktop,
}

pub fn select(
    surface: FixSurface,
    os: &SupportedOs,
    connection: &Connection,
    info: &mut ProviderInfo,
    cause: Option<FixCause>,
) -> Option<ProviderFixAction> {
    if !matches!(
        info.status,
        ProviderStatus::Degraded | ProviderStatus::Unavailable
    ) {
        return None;
    }
    if let Some(problem @ (ProviderProblem::Refused | ProviderProblem::ScopeMissing)) = info.problem
        && let Some(setup) = super::adapter(connection.source()).credential_setup()
    {
        let hint = setup.recovery_hint(problem);
        info.reason = Some(match info.reason.take() {
            Some(reason) => format!("{reason} {hint}"),
            None => hint,
        });
        return setup
            .saved(connection)
            .map(|action| ProviderFixAction::Web { action });
    }
    if surface != FixSurface::Desktop || *os != SupportedOs::Macos {
        return None;
    }
    let cause = cause.filter(|cause| info.problem == Some(cause.problem()))?;
    let target = match (connection.source(), cause) {
        (Source::Outlook, FixCause::AutomationDenied) => LocalFixTarget::SystemSettings {
            pane: SystemSettingsPane::Automation,
        },
        (Source::Git, FixCause::FolderDenied)
            if connection.location == ConnectionLocation::Local =>
        {
            LocalFixTarget::SystemSettings {
                pane: SystemSettingsPane::FilesAndFolders,
            }
        }
        (Source::Outlook, FixCause::AppNotRunning(LocalApp::Outlook)) => LocalFixTarget::OpenApp {
            app: LocalApp::Outlook,
        },
        _ => return None,
    };
    Some(ProviderFixAction::Local { target })
}

#[cfg(test)]
mod tests;
