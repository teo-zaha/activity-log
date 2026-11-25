use super::{FixCause, FixSurface, select};
use crate::{
    model::{
        ConnectionId, LocalApp, LocalFixTarget, ProviderFixAction, ProviderInfo, ProviderProblem,
        ProviderStatus, Source, SupportedOs, SystemSettingsPane, WebFixAction,
    },
    provider::connection::{Connection, ConnectionLocation},
};

#[test]
fn calendar_guides_use_the_approved_public_destinations_and_explain_link_access() {
    use crate::model::CalendarFeedService::*;

    let WebFixAction::FeedHelp { guides, recovery } = super::calendar::field_help() else {
        panic!("expected calendar help");
    };
    assert_eq!(recovery, None);
    let expected = [
        (OutlookWork, "https://outlook.office.com/calendar/", true),
        (OutlookCom, "https://outlook.live.com/calendar/", true),
        (
            GoogleCalendar,
            "https://calendar.google.com/calendar/u/0/r/settings",
            false,
        ),
        (ICloud, "https://www.icloud.com/calendar/", true),
        (
            Fastmail,
            "https://app.fastmail.com/settings/calendars?sessionpicker=1",
            true,
        ),
    ];
    assert_eq!(guides.len(), expected.len());
    for (guide, (service, url, publishes)) in guides.iter().zip(expected) {
        assert_eq!(guide.service, service);
        assert_eq!(guide.url.as_deref(), Some(url));
        assert!(guide.instructions.contains("apps you trust"));
        assert_eq!(
            guide.instructions.contains("Anyone with the link"),
            publishes
        );
    }
}

#[test]
fn calendar_unavailable_reuses_the_matched_guide_or_gives_generic_help_on_both_surfaces() {
    use crate::model::CalendarFeedService::{GoogleCalendar, OutlookCom, OutlookWork};

    let WebFixAction::FeedHelp {
        guides: catalog, ..
    } = super::calendar::field_help()
    else {
        panic!("expected calendar help");
    };
    let connection = Connection::new(
        ConnectionId::new(Source::Calendar, "work"),
        "Work",
        ConnectionLocation::Local,
        Default::default(),
    );
    let mut info = ProviderInfo {
        connection: connection.id.clone(),
        label: connection.label.clone(),
        wsl_distro: None,
        status: ProviderStatus::Unavailable,
        usable: false,
        supported_os: vec![SupportedOs::Linux],
        reason: Some("calendar.example.org answered 404 Not Found".into()),
        problem: Some(ProviderProblem::FeedUnavailable),
        fix_action: None,
        recheckable: true,
        checked_at: "2026-11-25T00:00:00Z".into(),
    };
    for surface in [FixSurface::Web, FixSurface::Desktop] {
        for (service, words) in [
            (Some(OutlookWork), "Publish it again"),
            (Some(OutlookCom), "Publish it again"),
            (Some(GoogleCalendar), "reset it first"),
            (None, "Check the calendar’s sharing settings"),
        ] {
            let result = select(
                surface,
                &SupportedOs::Linux,
                &connection,
                &mut info,
                Some(FixCause::FeedUnavailable(service)),
            );
            let Some(ProviderFixAction::Web {
                action: WebFixAction::FeedHelp { guides, recovery },
            }) = result
            else {
                panic!("expected a web recovery");
            };
            assert!(recovery.unwrap().contains(words));
            assert_eq!(
                guides,
                catalog
                    .iter()
                    .filter(|guide| Some(guide.service) == service)
                    .cloned()
                    .collect::<Vec<_>>()
            );
        }
    }
    assert_eq!(
        info.reason.as_deref(),
        Some("calendar.example.org answered 404 Not Found")
    );
    info.problem = Some(ProviderProblem::Throttled);
    assert_eq!(
        select(
            FixSurface::Web,
            &SupportedOs::Linux,
            &connection,
            &mut info,
            Some(FixCause::FeedUnavailable(None))
        ),
        None
    );
}

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
        let desktop = |connection: &Connection, info: &mut ProviderInfo, cause| {
            select(
                FixSurface::Desktop,
                &SupportedOs::Macos,
                connection,
                info,
                cause,
            )
        };
        assert_eq!(
            desktop(&connection, &mut info, Some(cause)),
            Some(ProviderFixAction::Local { target })
        );
        assert_eq!(desktop(&connection, &mut info, None), None);
        assert_eq!(
            select(
                FixSurface::Web,
                &SupportedOs::Macos,
                &connection,
                &mut info,
                Some(cause)
            ),
            None
        );
        assert_eq!(
            select(
                FixSurface::Desktop,
                &SupportedOs::Windows,
                &connection,
                &mut info,
                Some(cause)
            ),
            None
        );

        if source == Source::Git {
            connection.location = ConnectionLocation::Wsl("Example".into());
            assert_eq!(desktop(&connection, &mut info, Some(cause)), None);
            connection.location = ConnectionLocation::Local;
        }
        info.problem = Some(ProviderProblem::Throttled);
        assert_eq!(desktop(&connection, &mut info, Some(cause)), None);
        info.problem = None;
        assert_eq!(desktop(&connection, &mut info, Some(cause)), None);
        info.problem = Some(problem);
        for status in [ProviderStatus::Checking, ProviderStatus::Available] {
            info.status = status;
            assert_eq!(desktop(&connection, &mut info, Some(cause)), None);
        }
    }
}

#[test]
fn token_pages_use_the_provider_web_address_and_declared_permissions() {
    use std::collections::BTreeMap;

    let github = super::super::adapter(&Source::Github)
        .credential_setup()
        .unwrap();
    let WebFixAction::CreateToken {
        url,
        instructions,
        classic_url,
    } = github
        .action(Some("https://github.example.org:8443/?private=strip#local"))
        .unwrap()
    else {
        panic!("expected token setup");
    };
    let url = url::Url::parse(&url).unwrap();
    assert_eq!(
        url.origin().ascii_serialization(),
        "https://github.example.org:8443"
    );
    assert_eq!(url.path(), "/settings/personal-access-tokens/new");
    assert_eq!(url.fragment(), None);
    assert_eq!(
        url.query_pairs().into_owned().collect::<BTreeMap<_, _>>(),
        [
            ("name", "Activity Log"),
            ("description", "Read activity for Activity Log"),
            ("metadata", "read"),
            ("pull_requests", "read"),
            ("issues", "read"),
            ("contents", "read"),
            ("expires_in", "365"),
        ]
        .into_iter()
        .map(|(key, value)| (key.into(), value.into()))
        .collect()
    );
    let classic = url::Url::parse(classic_url.as_deref().unwrap()).unwrap();
    assert_eq!(classic.origin(), url.origin());
    assert_eq!(classic.path(), "/settings/tokens/new");
    assert_eq!(
        classic.query_pairs().collect::<BTreeMap<_, _>>(),
        [
            ("description".into(), "Activity Log".into()),
            ("scopes".into(), "repo".into()),
        ]
        .into()
    );
    for word in [
        "Metadata",
        "Pull requests",
        "Issues",
        "Contents",
        "one owner",
        "SSO",
    ] {
        assert!(instructions.contains(word), "{instructions}");
    }
    let fine_grained = "Include Metadata, Pull requests, Issues, Contents.";
    let classic = "The classic token grants broader access.";
    assert!(instructions.find(fine_grained).unwrap() < instructions.find(classic).unwrap());
    assert!(github.action(Some("http://github.com")).is_none());

    for (source, address, expected, words) in [
        (
            Source::Gitlab,
            Some("https://gitlab.example.org:8443/install/?private=strip#local"),
            "https://gitlab.example.org:8443/install/-/user_settings/personal_access_tokens?name=Activity+Log&description=Read+activity+for+Activity+Log&scopes=read_api",
            &["read_api", "Legacy token"][..],
        ),
        (
            Source::Gitlab,
            Some("https://gitlab.example.org"),
            "https://gitlab.example.org/-/user_settings/personal_access_tokens?name=Activity+Log&description=Read+activity+for+Activity+Log&scopes=read_api",
            &["read_api", "Legacy token"],
        ),
        (
            Source::Jira,
            Some("https://jira.example.org:8443/jira/?private=strip#local"),
            "https://jira.example.org:8443/jira/secure/ViewProfile.jspa",
            &["Personal access tokens", "account permissions"],
        ),
        (
            Source::Figma,
            None,
            "https://www.figma.com/settings",
            &[
                "90 days",
                "Security",
                "current_user:read",
                "file_versions:read",
                "file_comments:read",
                "file_metadata:read",
                "folders:read",
            ],
        ),
    ] {
        let setup = super::super::adapter(&source).credential_setup().unwrap();
        let Some(WebFixAction::CreateToken {
            url,
            instructions,
            classic_url,
        }) = setup.action(address)
        else {
            panic!("expected token setup for {source}");
        };
        assert_eq!(url, expected);
        assert_eq!(classic_url, None);
        for word in words {
            assert!(instructions.contains(word), "{instructions}");
        }
        if address.is_some() {
            assert!(setup.action(None).is_none());
            assert!(
                setup
                    .action(Some("https://user@host.example.org"))
                    .is_none()
            );
            assert!(setup.action(Some("file:///host.example.org")).is_none());
        }
    }
}

#[test]
fn token_recipes_match_the_registered_account_secrets_and_url_keys() {
    use crate::provider::{provider_config::ProviderConfigValueType, provider_registry};

    for provider in provider_registry() {
        let source = provider.id();
        let keys = provider.config_requirements();
        // A feed URL is private too, but it does not name a provider account.
        let account_secrets = keys
            .iter()
            .copied()
            .filter(|key| key.is_secret() && key.account)
            .collect::<Vec<_>>();
        let setup = provider.credential_setup();
        assert_eq!(setup.is_some(), !account_secrets.is_empty(), "{source}");
        if let Some(setup) = setup {
            assert_eq!(account_secrets, [setup.token], "{source}");
            let is_bound_url = |key: &crate::provider::provider_config::ProviderConfigKey| {
                key.binds && key.value_type() == ProviderConfigValueType::Url
            };
            match setup.address_key() {
                Some(key) => assert!(keys.contains(&key) && is_bound_url(&key), "{source}"),
                None => assert!(!keys.iter().any(is_bound_url), "{source}"),
            }
        }
    }
}

#[test]
fn multiple_token_scopes_are_one_query_parameter() {
    use super::credential::{TokenPage, TokenPermission};

    let page = TokenPage {
        path: &["tokens"],
        parameters: &[],
        permissions: &[
            TokenPermission::Scope("read_api"),
            TokenPermission::Scope("read_user"),
        ],
        expiry_days: None,
    };
    let url = page
        .url("https://gitlab.example.org".parse().unwrap())
        .unwrap();
    assert_eq!(
        url,
        "https://gitlab.example.org/tokens?scopes=read_api%2Cread_user"
    );
}

#[test]
fn credential_recovery_uses_the_same_permissions_on_web_and_desktop() {
    let config = crate::settings::ConfigFile::parse(
        "config.toml",
        b"[[gitlab]]\nurl = 'https://gitlab.example.org'\ntoken = 'example-secret'\n",
    )
    .unwrap();
    let connection = &config.connections().connections[0];
    for surface in [FixSurface::Web, FixSurface::Desktop] {
        for problem in [
            ProviderProblem::Refused,
            ProviderProblem::ScopeMissing,
            ProviderProblem::Throttled,
        ] {
            let mut info = ProviderInfo {
                connection: connection.id.clone(),
                label: connection.label.clone(),
                wsl_distro: None,
                status: ProviderStatus::Unavailable,
                usable: false,
                supported_os: vec![SupportedOs::Macos],
                reason: Some("Example failure".into()),
                problem: Some(problem),
                fix_action: None,
                recheckable: true,
                checked_at: "2026-11-25T00:00:00Z".into(),
            };
            let action = select(surface, &SupportedOs::Macos, connection, &mut info, None);
            if problem == ProviderProblem::Throttled {
                assert_eq!(action, None);
                assert_eq!(info.reason.as_deref(), Some("Example failure"));
                continue;
            }
            let Some(ProviderFixAction::Web {
                action: WebFixAction::CreateToken { url, .. },
            }) = action
            else {
                panic!("expected token recovery");
            };
            assert!(url.starts_with("https://gitlab.example.org/"));
            assert!(url.contains("scopes=read_api"));
            assert!(!url.contains("example-secret"));
            assert!(info.reason.as_deref().unwrap().contains("read_api"));
        }
    }
}
