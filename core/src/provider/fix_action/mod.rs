//! Selects an action for the outcome the host is about to publish.

use crate::model::{
    LocalApp, LocalFixTarget, ProviderFixAction, ProviderInfo, ProviderStatus, Source, SupportedOs,
    SystemSettingsPane,
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

pub(crate) fn select(
    surface: FixSurface,
    os: &SupportedOs,
    connection: &Connection,
    info: &ProviderInfo,
    cause: Option<FixCause>,
) -> Option<ProviderFixAction> {
    if !matches!(
        info.status,
        ProviderStatus::Degraded | ProviderStatus::Unavailable
    ) {
        return None;
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
