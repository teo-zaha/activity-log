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
        "export const SOURCES = [\"git\", \"gitlab\", \"jira\", \"outlook\", \"system\", \"calendar\"] as const satisfies readonly Source[]\n"
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
