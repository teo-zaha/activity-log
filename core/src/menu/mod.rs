//! The rules of the macOS menu the dashboard sends (`MenuSpec`): which menus can be installed,
//! and which updates an installed one takes. The desktop app builds and installs the native menu.

use std::collections::{HashMap, HashSet};

use crate::model::{MenuItemKey, MenuItemState, MenuItemUpdate, MenuKey, MenuSpec};

#[cfg(test)]
mod tests;

/// The desktop's event for a chosen menu item (`MenuCommand`).
pub const MENU_COMMAND_EVENT: &str = "menu-command";
/// The desktop command that installs a `MenuSpec` as the app's menu.
pub const INSTALL_MENU_COMMAND: &str = "install_menu";
/// The desktop command that changes what installed items show.
pub const UPDATE_MENU_COMMAND: &str = "update_menu";

/// Why a menu was not installed or not updated.
#[derive(Debug)]
pub enum MenuError {
    /// Two items of one menu have this key.
    DuplicateKey(MenuItemKey),
    /// Two items of one menu have this accelerator.
    DuplicateAccelerator(MenuKey),
    /// The update names a menu that is no longer installed: a later install, or the page that
    /// installed it loaded again.
    Stale { generation: u32 },
    /// The installed menu has no item with this key.
    UnknownItem(MenuItemKey),
    /// The update would turn a command into a checked item or back.
    KindChanged(MenuItemKey),
    /// The system did not build or install the menu.
    Native(anyhow::Error),
}

impl std::fmt::Display for MenuError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateKey(key) => write!(f, "Two menu items have the key {key}"),
            Self::DuplicateAccelerator(key) => {
                write!(
                    f,
                    "Two menu items have the accelerator {}",
                    key.accelerator()
                )
            }
            Self::Stale { generation } => {
                write!(f, "Menu {generation} is no longer the app's menu")
            }
            Self::UnknownItem(key) => write!(f, "The menu has no item {key}"),
            Self::KindChanged(key) => write!(f, "Menu item {key} cannot change its kind"),
            Self::Native(err) => write!(f, "The menu could not be built: {err}"),
        }
    }
}

impl std::error::Error for MenuError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Native(err) => Some(err.as_ref()),
            _ => None,
        }
    }
}

/// What the items of an installed menu show, by key.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MenuItems(HashMap<MenuItemKey, MenuItemState>);

impl MenuItems {
    /// The items of `spec`, if it can be installed: each key and each accelerator once.
    pub fn of(spec: &MenuSpec) -> Result<Self, MenuError> {
        let mut items = HashMap::new();
        let mut accelerators = HashSet::new();
        for item in spec.sections.iter().flat_map(|section| &section.items) {
            if items.insert(item.key, item.state.clone()).is_some() {
                return Err(MenuError::DuplicateKey(item.key));
            }
            if let Some(key) = item.accelerator
                && !accelerators.insert(key)
            {
                return Err(MenuError::DuplicateAccelerator(key));
            }
        }
        Ok(Self(items))
    }

    /// Checks `updates` against the items as they are, all or none: each names an item, keeps
    /// its kind, and is applied to what the items show only if every one fits.
    pub fn apply(&mut self, updates: &[MenuItemUpdate]) -> Result<(), MenuError> {
        for update in updates {
            let current = self
                .0
                .get(&update.key)
                .ok_or(MenuError::UnknownItem(update.key))?;
            if !current.same_kind(&update.state) {
                return Err(MenuError::KindChanged(update.key));
            }
        }
        for update in updates {
            self.0.insert(update.key, update.state.clone());
        }
        Ok(())
    }

    pub fn get(&self, key: MenuItemKey) -> Option<&MenuItemState> {
        self.0.get(&key)
    }
}
