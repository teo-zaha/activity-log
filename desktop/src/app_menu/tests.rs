use std::{cell::RefCell, rc::Rc};

use activity_log_core::{
    menu::{INSTALL_MENU_COMMAND, MenuError, MenuItems, UPDATE_MENU_COMMAND},
    model::{
        BackendError, BackendErrorKind, MenuItemKey, MenuItemSpec, MenuItemState, MenuItemUpdate,
        MenuKey, MenuSection, MenuSpec, MenuSubmenu,
    },
};
use muda::accelerator::{Accelerator, CMD_OR_CTRL, Code};
use strum::IntoEnumIterator;
use tauri::{
    ipc::{CallbackFn, InvokeBody},
    menu::MenuId,
    test::{INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder, mock_context, noop_assets},
    webview::InvokeRequest,
};

use super::{AppMenu, BuiltItem, Menus, NativeItem, Slot, SubmenuLayout, item_id, layout};

fn command(key: u32, title: &str, accelerator: Option<MenuKey>) -> MenuItemSpec {
    MenuItemSpec {
        key: MenuItemKey(key),
        accelerator,
        state: MenuItemState::Command {
            title: title.into(),
            enabled: true,
        },
    }
}

fn check(key: u32, title: &str, accelerator: MenuKey) -> MenuItemSpec {
    MenuItemSpec {
        key: MenuItemKey(key),
        accelerator: Some(accelerator),
        state: MenuItemState::Check {
            title: title.into(),
            enabled: true,
            checked: false,
        },
    }
}

fn section(submenu: MenuSubmenu, items: Vec<MenuItemSpec>) -> MenuSection {
    MenuSection { submenu, items }
}

/// The dashboard's menu as its shortcut table makes it.
fn dashboard_spec() -> MenuSpec {
    MenuSpec {
        sections: vec![
            section(
                MenuSubmenu::App,
                vec![command(0, "Settings…", Some(MenuKey::Comma))],
            ),
            section(
                MenuSubmenu::File,
                vec![
                    command(1, "Collect", Some(MenuKey::R)),
                    command(2, "Export Markdown…", None),
                    command(3, "Export JSON…", None),
                ],
            ),
            section(
                MenuSubmenu::Edit,
                vec![command(4, "Filter Activity…", Some(MenuKey::F))],
            ),
            section(
                MenuSubmenu::View,
                vec![
                    check(5, "Timeline", MenuKey::Digit1),
                    check(6, "Day", MenuKey::Digit2),
                ],
            ),
            section(
                MenuSubmenu::View,
                vec![command(7, "Back", Some(MenuKey::BracketLeft))],
            ),
            section(
                MenuSubmenu::Help,
                vec![command(8, "Keyboard Shortcuts", None)],
            ),
        ],
    }
}

/// A submenu's places as names: system items, `-` for a separator, the app's by title.
fn names(submenu: &SubmenuLayout<'_>) -> Vec<String> {
    submenu
        .slots
        .iter()
        .map(|slot| match slot {
            Slot::System(item) => format!("{item:?}"),
            Slot::Separator => "-".into(),
            Slot::App(item) => item.state.title().into(),
        })
        .collect()
}

fn submenu<'a>(bar: &'a [SubmenuLayout<'a>], which: MenuSubmenu) -> Vec<String> {
    names(
        bar.iter()
            .find(|s| s.submenu == which)
            .expect("the submenu"),
    )
}

#[test]
fn lays_out_the_bar_with_the_system_items_around_the_apps() {
    let spec = dashboard_spec();
    let bar = layout(&spec);

    assert_eq!(
        bar.iter().map(|s| s.submenu).collect::<Vec<_>>(),
        MenuSubmenu::iter().collect::<Vec<_>>()
    );
    assert_eq!(
        submenu(&bar, MenuSubmenu::App),
        [
            "About",
            "-",
            "Settings…",
            "-",
            "Services",
            "-",
            "Hide",
            "HideOthers",
            "ShowAll",
            "-",
            "Quit"
        ]
    );
    assert_eq!(
        submenu(&bar, MenuSubmenu::File),
        [
            "Collect",
            "Export Markdown…",
            "Export JSON…",
            "-",
            "CloseWindow"
        ]
    );
    // The system's editing items stay, or ⌘C and ⌘V do nothing in a field.
    assert_eq!(
        submenu(&bar, MenuSubmenu::Edit),
        [
            "Undo",
            "Redo",
            "-",
            "Cut",
            "Copy",
            "Paste",
            "SelectAll",
            "-",
            "Filter Activity…"
        ]
    );
    assert_eq!(
        submenu(&bar, MenuSubmenu::View),
        ["Timeline", "Day", "-", "Back", "-", "Fullscreen"]
    );
    assert_eq!(
        submenu(&bar, MenuSubmenu::Window),
        ["Minimize", "Zoom", "-", "BringAllToFront"]
    );
    assert_eq!(submenu(&bar, MenuSubmenu::Help), ["Keyboard Shortcuts"]);
}

#[test]
fn leaves_out_an_empty_submenu_and_empty_sections() {
    let spec = MenuSpec {
        sections: vec![
            section(MenuSubmenu::View, vec![]),
            section(MenuSubmenu::View, vec![command(1, "Back", None)]),
            section(MenuSubmenu::Help, vec![]),
        ],
    };
    let bar = layout(&spec);

    assert!(bar.iter().all(|s| s.submenu != MenuSubmenu::Help));
    assert_eq!(
        submenu(&bar, MenuSubmenu::View),
        ["Back", "-", "Fullscreen"]
    );
    assert_eq!(
        submenu(&bar, MenuSubmenu::Edit).last().unwrap(),
        "SelectAll"
    );
    assert_eq!(submenu(&bar, MenuSubmenu::File), ["CloseWindow"]);
}

/// The parse test below checks Tauri's own parser only while the lock holds one `muda`, the one
/// Tauri's menu uses; a Tauri that moves to another one fails here until the dev-dependency follows.
#[test]
fn the_accelerator_parser_checked_is_the_one_tauri_uses() {
    let lock = include_str!("../../../Cargo.lock");
    let packages = lock
        .lines()
        .filter(|line| *line == "name = \"muda\"")
        .count();
    assert_eq!(packages, 1, "Cargo.lock holds {packages} muda packages");
}

#[test]
fn every_menu_key_is_an_accelerator_of_the_command_key_and_that_key() {
    let code = |key: MenuKey| match key {
        MenuKey::R => Code::KeyR,
        MenuKey::F => Code::KeyF,
        MenuKey::BracketLeft => Code::BracketLeft,
        MenuKey::Comma => Code::Comma,
        MenuKey::Digit1 => Code::Digit1,
        MenuKey::Digit2 => Code::Digit2,
        MenuKey::Digit3 => Code::Digit3,
        MenuKey::Digit4 => Code::Digit4,
    };
    for key in MenuKey::iter() {
        let parsed: Accelerator = key.accelerator().parse().unwrap_or_else(|err| {
            panic!("{key:?}: {err}");
        });
        assert_eq!(parsed, Accelerator::new(CMD_OR_CTRL, code(key)), "{key:?}");
    }
}

/// A native item that records what it was told to show.
#[derive(Clone, Default)]
struct Recorded(Rc<RefCell<Vec<MenuItemState>>>);

impl NativeItem for Recorded {
    fn show(&self, state: &MenuItemState) -> tauri::Result<()> {
        self.0.borrow_mut().push(state.clone());
        Ok(())
    }
}

fn built(generation: u32, spec: &MenuSpec, shown: &Recorded) -> Vec<BuiltItem<Recorded>> {
    spec.sections
        .iter()
        .flat_map(|section| &section.items)
        .map(|item| BuiltItem {
            key: item.key,
            id: item_id(generation, item.key),
            handle: shown.clone(),
        })
        .collect()
}

#[test]
fn reports_a_chosen_item_by_its_key_and_the_generation_of_its_menu() {
    let menus = Menus::<Recorded>::default();
    let spec = dashboard_spec();
    let shown = Recorded::default();

    let generation = menus
        .install(MenuItems::of(&spec).unwrap(), |generation| {
            Ok(built(generation, &spec, &shown))
        })
        .unwrap();

    let chosen = menus.chosen(&item_id(generation, MenuItemKey(1))).unwrap();
    assert_eq!(
        (chosen.generation, chosen.key),
        (generation, MenuItemKey(1))
    );
    // A system item, and one of an older menu, are not the dashboard's.
    assert!(menus.chosen(&MenuId::new("quit")).is_none());
    assert!(
        menus
            .chosen(&item_id(generation + 1, MenuItemKey(1)))
            .is_none()
    );
}

#[test]
fn keeps_the_menu_before_when_the_system_does_not_install_one() {
    let menus = Menus::<Recorded>::default();
    let spec = dashboard_spec();
    let shown = Recorded::default();
    let first = menus
        .install(MenuItems::of(&spec).unwrap(), |generation| {
            Ok(built(generation, &spec, &shown))
        })
        .unwrap();

    let failed = menus.install(MenuItems::of(&spec).unwrap(), |_| {
        Err(anyhow::anyhow!("no menu bar"))
    });

    assert!(matches!(failed, Err(MenuError::Native(_))));
    assert!(menus.chosen(&item_id(first, MenuItemKey(1))).is_some());
    let update = MenuItemUpdate {
        key: MenuItemKey(1),
        state: MenuItemState::Command {
            title: "Collect".into(),
            enabled: false,
        },
    };
    menus.update(first, std::slice::from_ref(&update)).unwrap();
    assert_eq!(*shown.0.borrow(), [update.state]);
}

#[test]
fn updates_only_the_menu_that_is_installed() {
    let menus = Menus::<Recorded>::default();
    let spec = dashboard_spec();
    let shown = Recorded::default();
    let day = |checked| MenuItemUpdate {
        key: MenuItemKey(6),
        state: MenuItemState::Check {
            title: "Day".into(),
            enabled: true,
            checked,
        },
    };
    assert!(matches!(
        menus.update(1, &[day(true)]),
        Err(MenuError::Stale { generation: 1 })
    ));

    let first = menus
        .install(MenuItems::of(&spec).unwrap(), |generation| {
            Ok(built(generation, &spec, &shown))
        })
        .unwrap();
    let second = menus
        .install(MenuItems::of(&spec).unwrap(), |generation| {
            Ok(built(generation, &spec, &shown))
        })
        .unwrap();
    assert!(second > first);
    assert!(matches!(
        menus.update(first, &[day(true)]),
        Err(MenuError::Stale { .. })
    ));
    menus.update(second, &[day(true)]).unwrap();
    assert_eq!(*shown.0.borrow(), [day(true).state]);

    // The page that installed it loaded again: neither its updates nor its clicks count.
    menus.retire();
    assert!(matches!(
        menus.update(second, &[day(false)]),
        Err(MenuError::Stale { .. })
    ));
    assert!(menus.chosen(&item_id(second, MenuItemKey(6))).is_none());
}

/// Runs `command` over IPC as the dashboard does: the answer, or the `BackendError` it refused.
fn invoke(command: &str, body: serde_json::Value) -> Result<serde_json::Value, BackendError> {
    let app = mock_builder()
        .manage(AppMenu::<MockRuntime>::default())
        .invoke_handler(tauri::generate_handler![
            super::install_menu,
            super::update_menu
        ])
        .build(mock_context(noop_assets()))
        .unwrap();
    let webview =
        tauri::WebviewWindowBuilder::<MockRuntime, _>::new(&app, "main", Default::default())
            .build()
            .unwrap();
    get_ipc_response(
        &webview,
        InvokeRequest {
            cmd: command.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: webview.url().unwrap(),
            body: InvokeBody::Json(body),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        },
    )
    .map(|answer| answer.deserialize().unwrap())
    .map_err(|rejected| serde_json::from_value(rejected).expect("a BackendError"))
}

/// The commands answer by the names and arguments the generated contract gives the dashboard.
#[test]
fn the_menu_commands_refuse_what_does_not_fit_by_kind() {
    let twice = MenuSpec {
        sections: vec![section(
            MenuSubmenu::File,
            vec![command(1, "Collect", None), command(1, "Export", None)],
        )],
    };
    let refused = invoke(INSTALL_MENU_COMMAND, serde_json::json!({ "spec": twice })).unwrap_err();
    assert_eq!(refused.kind, BackendErrorKind::InvalidRequest);

    let update = MenuItemUpdate {
        key: MenuItemKey(1),
        state: MenuItemState::Command {
            title: "Collect".into(),
            enabled: true,
        },
    };
    let stale = invoke(
        UPDATE_MENU_COMMAND,
        serde_json::json!({ "generation": 1, "items": [update] }),
    )
    .unwrap_err();
    assert_eq!(stale.kind, BackendErrorKind::Conflict);
}
