use super::*;
use crate::model::{MenuItemSpec, MenuSection, MenuSubmenu};

fn command(key: u32, accelerator: Option<MenuKey>) -> MenuItemSpec {
    MenuItemSpec {
        key: MenuItemKey(key),
        accelerator,
        state: MenuItemState::Command {
            title: format!("Item {key}"),
            enabled: true,
        },
    }
}

fn check(key: u32, checked: bool) -> MenuItemSpec {
    MenuItemSpec {
        key: MenuItemKey(key),
        accelerator: None,
        state: MenuItemState::Check {
            title: format!("View {key}"),
            enabled: true,
            checked,
        },
    }
}

fn spec(sections: Vec<(MenuSubmenu, Vec<MenuItemSpec>)>) -> MenuSpec {
    MenuSpec {
        sections: sections
            .into_iter()
            .map(|(submenu, items)| MenuSection { submenu, items })
            .collect(),
    }
}

#[test]
fn refuses_a_key_or_an_accelerator_twice_in_one_menu() {
    let keys = spec(vec![
        (MenuSubmenu::File, vec![command(1, None)]),
        (MenuSubmenu::View, vec![command(1, None)]),
    ]);
    assert!(matches!(
        MenuItems::of(&keys),
        Err(MenuError::DuplicateKey(MenuItemKey(1)))
    ));

    let accelerators = spec(vec![(
        MenuSubmenu::File,
        vec![command(1, Some(MenuKey::R)), command(2, Some(MenuKey::R))],
    )]);
    assert!(matches!(
        MenuItems::of(&accelerators),
        Err(MenuError::DuplicateAccelerator(MenuKey::R))
    ));

    let fine = spec(vec![(
        MenuSubmenu::File,
        vec![
            command(1, Some(MenuKey::R)),
            command(2, None),
            command(3, None),
        ],
    )]);
    assert!(MenuItems::of(&fine).is_ok());
}

#[test]
fn applies_an_update_only_if_every_item_is_known_and_keeps_its_kind() {
    let mut items = MenuItems::of(&spec(vec![(
        MenuSubmenu::View,
        vec![command(1, None), check(2, false)],
    )]))
    .unwrap();
    let checked = MenuItemUpdate {
        key: MenuItemKey(2),
        state: check(2, true).state,
    };

    let unknown = MenuItemUpdate {
        key: MenuItemKey(9),
        state: command(9, None).state,
    };
    assert!(matches!(
        items.apply(&[checked.clone(), unknown]),
        Err(MenuError::UnknownItem(MenuItemKey(9)))
    ));
    let turned = MenuItemUpdate {
        key: MenuItemKey(1),
        state: check(1, true).state,
    };
    assert!(matches!(
        items.apply(&[checked.clone(), turned]),
        Err(MenuError::KindChanged(MenuItemKey(1)))
    ));
    // Neither refused update changed anything.
    assert_eq!(items.get(MenuItemKey(2)), Some(&check(2, false).state));

    items.apply(&[checked]).unwrap();
    assert_eq!(items.get(MenuItemKey(2)), Some(&check(2, true).state));
}
