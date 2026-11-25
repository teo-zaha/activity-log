use super::activity_typescript;
use strum::IntoEnumIterator;

use crate::model::{CollectOptions, MaximizeButtonState, Source};

#[test]
fn holds_the_range_limit_the_backend_checks() {
    let output = activity_typescript();

    assert!(output.contains(&format!(
        "export const COLLECT_MAX_DAYS = {}\n",
        CollectOptions::MAX_DAYS
    )));
    assert!(output.ends_with('\n') && !output.ends_with("\n\n"));
}

#[test]
fn lists_every_source_by_its_wire_name() {
    assert!(activity_typescript().contains(
        "export const SOURCES = [\"git\", \"gitlab\", \"github\", \"jira\", \"outlook\", \"system\", \"calendar\", \"figma\"] as const satisfies readonly Source[]\n"
    ));
}

/// Config files, URLs and `ConnectionId`s spell a source the strum way, the wire the serde way:
/// one name for both.
#[test]
fn every_source_has_one_name() {
    for source in Source::iter() {
        let wire = serde_json::to_value(&source).unwrap();
        assert_eq!(wire, serde_json::Value::from(source.as_ref()), "{source:?}");
        assert_eq!(source.as_ref().parse::<Source>().unwrap(), source);
    }
}

/// The desktop emits this payload and the dashboard reads it as generated.
#[test]
fn the_maximize_button_payload_keeps_its_shape() {
    let payload = MaximizeButtonState {
        hovered: true,
        pressed: false,
    };
    assert_eq!(
        serde_json::to_string(&payload).unwrap(),
        r#"{"hovered":true,"pressed":false}"#
    );
    assert!(
        activity_typescript()
            .contains("export type MaximizeButtonState = { hovered: boolean, pressed: boolean, };")
    );
}

#[test]
fn provider_fix_actions_keep_the_generated_wire_shape() {
    use crate::model::{
        CalendarFeedGuide, CalendarFeedService, ConnectionId, LocalFixTarget, OpenLocalFixRequest,
        ProviderFixAction, ProviderProblem, Source, SystemSettingsPane, WebFixAction,
    };
    use serde_json::json;

    let local = ProviderFixAction::Local {
        target: LocalFixTarget::SystemSettings {
            pane: SystemSettingsPane::FilesAndFolders,
        },
    };
    assert_eq!(
        serde_json::to_value(local).unwrap(),
        json!({
            "kind": "local", "target": {"kind": "systemSettings", "pane": "filesAndFolders"},
        })
    );
    let open = OpenLocalFixRequest {
        connection: ConnectionId::new(Source::Outlook, "work"),
        target: LocalFixTarget::SystemSettings {
            pane: SystemSettingsPane::Automation,
        },
    };
    assert_eq!(
        serde_json::to_string(&open).unwrap(),
        r#"{"connection":{"source":"outlook","name":"work"},"target":{"kind":"systemSettings","pane":"automation"}}"#
    );
    let token = |classic_url: Option<&str>| ProviderFixAction::Web {
        action: WebFixAction::CreateToken {
            url: "https://example.org/tokens/new".into(),
            instructions: "Select read access.".into(),
            classic_url: classic_url.map(str::to_owned),
        },
    };
    let mut wire = json!({"kind": "web", "action": {
        "kind": "createToken", "url": "https://example.org/tokens/new",
        "instructions": "Select read access.",
    }});
    assert_eq!(serde_json::to_value(token(None)).unwrap(), wire);
    wire["action"]["classicUrl"] = json!("https://example.org/classic-tokens/new");
    assert_eq!(
        serde_json::to_value(token(Some("https://example.org/classic-tokens/new"))).unwrap(),
        wire
    );
    let feed = WebFixAction::FeedHelp {
        guides: vec![CalendarFeedGuide {
            service: CalendarFeedService::ICloud,
            label: "iCloud".into(),
            instructions: "Share the calendar.".into(),
            url: None,
        }],
        recovery: None,
    };
    assert_eq!(
        serde_json::to_value(feed).unwrap(),
        json!({"kind": "feedHelp", "guides": [
            {"service": "iCloud", "label": "iCloud", "instructions": "Share the calendar."}
        ]})
    );
    assert_eq!(
        serde_json::to_value(ProviderProblem::ScopeMissing).unwrap(),
        json!("scopeMissing")
    );
    assert_eq!(
        serde_json::to_string(&ProviderProblem::FeedUnavailable).unwrap(),
        r#""feedUnavailable""#
    );
    let recovery = ProviderFixAction::Web {
        action: WebFixAction::FeedHelp {
            guides: vec![],
            recovery: Some("Paste the current ICS link.".into()),
        },
    };
    assert_eq!(
        serde_json::to_string(&recovery).unwrap(),
        r#"{"kind":"web","action":{"kind":"feedHelp","guides":[],"recovery":"Paste the current ICS link."}}"#
    );
    let ts = activity_typescript();
    for shape in [
        r#"export type ProviderFixAction = { "kind": "web", action: WebFixAction, } | { "kind": "local", target: LocalFixTarget, };"#,
        "fixAction?: ProviderFixAction",
        "export type OpenLocalFixRequest = { connection: ConnectionId, target: LocalFixTarget, };",
        "fixAction?: WebFixAction",
        "fieldFix?: ProviderConfigFieldFixDto",
        "classicUrl?: string",
        "\"scopeMissing\"",
        "\"feedUnavailable\"",
        "\"iCloud\"",
    ] {
        assert!(ts.contains(shape), "{shape}");
    }
}

/// The desktop's menu commands and event, by the names and shapes the dashboard reads.
#[test]
fn names_the_menu_commands_and_event_and_keeps_their_shapes() {
    use crate::model::{MenuCommand, MenuInstall, MenuItemKey, MenuItemState};

    let output = activity_typescript();
    assert!(output.contains("export const MENU_COMMAND_EVENT = \"menu-command\"\n"));
    assert!(output.contains(
        "export const MENU_SUBMENUS = [\"app\", \"file\", \"edit\", \"view\", \"window\", \"help\"] as const satisfies readonly MenuSubmenu[]\n"
    ));
    assert!(output.contains(
        "export const MENU_COMMANDS = { install: \"install_menu\", update: \"update_menu\" } as const\n"
    ));
    assert_eq!(
        serde_json::to_value(MenuCommand {
            generation: 3,
            key: MenuItemKey(7)
        })
        .unwrap(),
        serde_json::json!({ "generation": 3, "key": 7 })
    );
    assert_eq!(
        serde_json::to_value(MenuInstall::Installed { generation: 1 }).unwrap(),
        serde_json::json!({ "kind": "installed", "generation": 1 })
    );
    assert_eq!(
        serde_json::to_value(MenuInstall::NotOffered).unwrap(),
        serde_json::json!({ "kind": "notOffered" })
    );
    assert_eq!(
        serde_json::to_value(MenuItemState::Check {
            title: "Day".into(),
            enabled: true,
            checked: false
        })
        .unwrap(),
        serde_json::json!({ "kind": "check", "title": "Day", "enabled": true, "checked": false })
    );
}

/// The desktop's own commands and events, and the events of the host's state, by the names the
/// desktop registers and the dashboard calls.
#[test]
fn names_the_desktop_commands_and_events() {
    let output = activity_typescript();
    assert!(output.contains(
        "export const DESKTOP_COMMANDS = { getRuntimeState: \"get_runtime_state\", collect: \"collect\","
    ));
    assert!(output.contains(
        "export const DESKTOP_EVENTS = { deepLink: \"deep-link\", maximizeButton: \"maximize-button\" } as const\n"
    ));
    assert!(output.contains(
        "export const RUNTIME_EVENTS = [\"state\", \"activity\", \"providers\", \"ping\"] as const\n"
    ));
}
