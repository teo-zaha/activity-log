use activity_log_core::model::{
    BackendErrorKind, ConnectionId, LocalApp, LocalFixTarget, OpenLocalFixRequest,
    ProviderFixAction, ProviderInfo, ProviderStatus, Source, SystemSettingsPane,
};

use super::{authorize, decode};

const AUTOMATION: LocalFixTarget = LocalFixTarget::SystemSettings {
    pane: SystemSettingsPane::Automation,
};

#[test]
fn local_fix_decoding_rejects_arbitrary_urls_and_arguments() {
    use serde_json::json;
    use tauri::ipc::InvokeBody;

    let request = |target| {
        InvokeBody::Json(json!({ "request": {
        "connection": { "source": "outlook", "name": "work" },
        "target": target,
    } }))
    };
    assert_eq!(
        decode(&request(
            json!({ "kind": "systemSettings", "pane": "automation" })
        ))
        .unwrap()
        .target,
        AUTOMATION
    );
    for target in [
        json!({ "kind": "openApp", "app": "outlook", "arguments": ["--example"] }),
        json!({ "kind": "systemSettings", "pane": "automation", "url": "https://example.org" }),
        json!({ "kind": "openUrl", "url": "https://example.org" }),
    ] {
        assert_eq!(
            decode(&request(target)).unwrap_err().kind,
            BackendErrorKind::InvalidRequest
        );
    }
}

#[test]
fn only_the_current_action_for_the_connection_is_authorized() {
    let connection = ConnectionId::new(Source::Outlook, "work");
    let request = OpenLocalFixRequest {
        connection: connection.clone(),
        target: AUTOMATION,
    };
    let mut provider = ProviderInfo {
        connection,
        label: "Outlook".into(),
        status: ProviderStatus::Degraded,
        usable: false,
        supported_os: Vec::new(),
        reason: None,
        problem: None,
        fix_action: Some(ProviderFixAction::Local { target: AUTOMATION }),
        recheckable: true,
        checked_at: String::new(),
        wsl_distro: None,
    };
    assert!(authorize("main", &request, std::slice::from_ref(&provider)).is_ok());
    for (window, target, name) in [
        ("other", AUTOMATION, "work"),
        ("main", AUTOMATION, "other"),
        (
            "main",
            LocalFixTarget::OpenApp {
                app: LocalApp::Outlook,
            },
            "work",
        ),
        (
            "main",
            LocalFixTarget::SystemSettings {
                pane: SystemSettingsPane::FilesAndFolders,
            },
            "work",
        ),
    ] {
        let request = OpenLocalFixRequest {
            connection: ConnectionId::new(Source::Outlook, name),
            target,
        };
        assert_eq!(
            authorize(window, &request, std::slice::from_ref(&provider))
                .unwrap_err()
                .kind,
            BackendErrorKind::InvalidRequest
        );
    }
    provider.fix_action = None;
    assert_eq!(
        authorize("main", &request, &[provider]).unwrap_err().kind,
        BackendErrorKind::InvalidRequest
    );
}

#[cfg(not(target_os = "macos"))]
#[tokio::test]
async fn local_recovery_is_refused_on_other_platforms() {
    assert_eq!(
        super::imp::open(AUTOMATION).await.unwrap_err().kind,
        BackendErrorKind::InvalidRequest
    );
}

#[cfg(target_os = "macos")]
#[test]
fn macos_recovery_uses_only_the_two_panes_and_the_outlook_bundle() {
    for (target, arguments) in [
        (
            AUTOMATION,
            &[
                "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_Automation",
            ][..],
        ),
        (
            LocalFixTarget::SystemSettings {
                pane: SystemSettingsPane::FilesAndFolders,
            },
            &[
                "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_FilesAndFolders",
            ],
        ),
        (
            LocalFixTarget::OpenApp {
                app: LocalApp::Outlook,
            },
            &["-b", "com.microsoft.Outlook"],
        ),
    ] {
        let command = super::macos::command(target);
        assert_eq!(command.get_program(), "/usr/bin/open");
        assert_eq!(command.get_args().collect::<Vec<_>>(), arguments);
    }
}

#[test]
fn desktop_capabilities_grant_no_opener_or_shell_access_to_the_page() {
    let capability: serde_json::Value =
        serde_json::from_str(include_str!("../../capabilities/default.json")).unwrap();
    for grant in capability["permissions"].as_array().unwrap() {
        let id = grant
            .as_str()
            .or_else(|| grant["identifier"].as_str())
            .unwrap();
        assert!(
            !id.starts_with("opener:") && !id.starts_with("shell:"),
            "{id}"
        );
    }
}
