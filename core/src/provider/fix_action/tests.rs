use super::{FixCause, FixSurface, select};
use crate::{
    model::{
        ConnectionId, LocalApp, LocalFixTarget, ProviderFixAction, ProviderInfo, ProviderProblem,
        ProviderStatus, Source, SupportedOs, SystemSettingsPane,
    },
    provider::connection::{Connection, ConnectionLocation},
};

#[test]
fn native_recovery_requires_a_matching_problem_on_the_macos_desktop() {
    for (source, problem, cause, target) in [
        (
            Source::Outlook,
            ProviderProblem::PermissionDenied,
            FixCause::AutomationDenied,
            LocalFixTarget::SystemSettings {
                pane: SystemSettingsPane::Automation,
            },
        ),
        (
            Source::Outlook,
            ProviderProblem::NotRunning,
            FixCause::AppNotRunning(LocalApp::Outlook),
            LocalFixTarget::OpenApp {
                app: LocalApp::Outlook,
            },
        ),
        (
            Source::Git,
            ProviderProblem::PermissionDenied,
            FixCause::FolderDenied,
            LocalFixTarget::SystemSettings {
                pane: SystemSettingsPane::FilesAndFolders,
            },
        ),
    ] {
        let mut connection = Connection::new(
            ConnectionId::new(source.clone(), "work"),
            "Work",
            ConnectionLocation::Local,
            Default::default(),
        );
        let mut info = ProviderInfo {
            connection: connection.id.clone(),
            label: connection.label.clone(),
            wsl_distro: None,
            status: ProviderStatus::Degraded,
            usable: true,
            supported_os: vec![SupportedOs::Macos],
            reason: Some("Example failure".into()),
            problem: Some(problem),
            fix_action: None,
            recheckable: true,
            checked_at: "2026-11-25T00:00:00Z".into(),
        };
        let desktop = |connection: &Connection, info: &ProviderInfo, cause| {
            select(
                FixSurface::Desktop,
                &SupportedOs::Macos,
                connection,
                info,
                cause,
            )
        };
        assert_eq!(
            desktop(&connection, &info, Some(cause)),
            Some(ProviderFixAction::Local { target })
        );
        assert_eq!(desktop(&connection, &info, None), None);
        assert_eq!(
            select(
                FixSurface::Web,
                &SupportedOs::Macos,
                &connection,
                &info,
                Some(cause)
            ),
            None
        );
        assert_eq!(
            select(
                FixSurface::Desktop,
                &SupportedOs::Windows,
                &connection,
                &info,
                Some(cause)
            ),
            None
        );

        if source == Source::Git {
            connection.location = ConnectionLocation::Wsl("Example".into());
            assert_eq!(desktop(&connection, &info, Some(cause)), None);
            connection.location = ConnectionLocation::Local;
        }
        info.problem = Some(ProviderProblem::Throttled);
        assert_eq!(desktop(&connection, &info, Some(cause)), None);
        info.problem = None;
        assert_eq!(desktop(&connection, &info, Some(cause)), None);
        info.problem = Some(problem);
        for status in [ProviderStatus::Checking, ProviderStatus::Available] {
            info.status = status;
            assert_eq!(desktop(&connection, &info, Some(cause)), None);
        }
    }
}
