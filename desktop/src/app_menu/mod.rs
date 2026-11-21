//! The macOS menu bar. The dashboard builds its part from its shortcut table
//! (`models/shortcuts.ts`) and sends it as a `MenuSpec`; this module puts the system's items
//! around it (`layout`), installs it as the app's menu (`macos`), changes what its items show,
//! and reports a chosen item as the `menu-command` event. Windows and Linux have no menu bar:
//! `install_menu` answers `notOffered` there and the dashboard keeps every key.
//!
//! Items are named by the dashboard's numeric keys and the generation of the menu they belong
//! to, so a click on a menu that a later install or a reload replaced is never reported.

#[cfg(target_os = "macos")]
mod macos;

#[cfg(test)]
mod tests;

use std::{collections::HashMap, sync::Mutex};

use activity_log_core::{
    diagnostics,
    menu::{MENU_COMMAND_EVENT, MenuError, MenuItems},
    model::{
        BackendError, MenuCommand, MenuInstall, MenuItemKey, MenuItemSpec, MenuItemState,
        MenuItemUpdate, MenuSection, MenuSpec, MenuSubmenu,
    },
};
use tauri::{
    AppHandle, Emitter, Manager, Runtime, State,
    menu::{CheckMenuItem, MenuId, MenuItem},
};

use crate::deep_link::MAIN_WINDOW;

/// The Logs pane's name for what the menu reports.
pub(crate) const LOG_SCOPE: &str = "menu";

/// An item the system provides, with its own title, accelerator and action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) enum SystemItem {
    About,
    Services,
    Hide,
    HideOthers,
    ShowAll,
    Quit,
    CloseWindow,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
    Fullscreen,
    Minimize,
    Zoom,
    BringAllToFront,
}

/// One place in a submenu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Slot<'a> {
    System(SystemItem),
    Separator,
    App(&'a MenuItemSpec),
}

/// A submenu of the bar and what it holds, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SubmenuLayout<'a> {
    pub submenu: MenuSubmenu,
    pub slots: Vec<Slot<'a>>,
}

/// The menu bar for `spec`: the system's items where macOS has them, the app's sections where
/// they belong, a separator between groups. A submenu with nothing in it is left out. Edit keeps
/// the system's editing items, without which ⌘C, ⌘V and ⌘A do nothing in a field.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn layout(spec: &MenuSpec) -> Vec<SubmenuLayout<'_>> {
    use SystemItem::*;
    let sections = |submenu: MenuSubmenu| -> Vec<&MenuSection> {
        spec.sections
            .iter()
            .filter(|section| section.submenu == submenu && !section.items.is_empty())
            .collect()
    };
    let system = |items: &[SystemItem]| items.iter().map(|&item| Slot::System(item)).collect();
    let bar = [
        (
            MenuSubmenu::App,
            vec![
                system(&[About]),
                app_slots(sections(MenuSubmenu::App)),
                system(&[Services]),
                system(&[Hide, HideOthers, ShowAll]),
                system(&[Quit]),
            ],
        ),
        (
            MenuSubmenu::File,
            vec![
                app_slots(sections(MenuSubmenu::File)),
                system(&[CloseWindow]),
            ],
        ),
        (
            MenuSubmenu::Edit,
            vec![
                system(&[Undo, Redo]),
                system(&[Cut, Copy, Paste, SelectAll]),
                app_slots(sections(MenuSubmenu::Edit)),
            ],
        ),
        (
            MenuSubmenu::View,
            vec![
                app_slots(sections(MenuSubmenu::View)),
                system(&[Fullscreen]),
            ],
        ),
        (
            MenuSubmenu::Window,
            vec![
                system(&[Minimize, Zoom]),
                app_slots(sections(MenuSubmenu::Window)),
                system(&[BringAllToFront]),
            ],
        ),
        (
            MenuSubmenu::Help,
            vec![app_slots(sections(MenuSubmenu::Help))],
        ),
    ];
    bar.into_iter()
        .filter_map(|(submenu, groups)| {
            let slots = separated(groups);
            (!slots.is_empty()).then_some(SubmenuLayout { submenu, slots })
        })
        .collect()
}

/// The app's sections of one submenu, a separator between two.
fn app_slots(sections: Vec<&MenuSection>) -> Vec<Slot<'_>> {
    separated(
        sections
            .into_iter()
            .map(|section| section.items.iter().map(Slot::App).collect())
            .collect(),
    )
}

/// The non-empty groups in order, a separator between two.
fn separated(groups: Vec<Vec<Slot<'_>>>) -> Vec<Slot<'_>> {
    let mut slots = Vec::new();
    for group in groups.into_iter().filter(|group| !group.is_empty()) {
        if !slots.is_empty() {
            slots.push(Slot::Separator);
        }
        slots.extend(group);
    }
    slots
}

/// What a native item shows can be changed.
pub(crate) trait NativeItem {
    fn show(&self, state: &MenuItemState) -> tauri::Result<()>;
}

/// An installed item of the app's own.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) enum Handle<R: Runtime> {
    Command(MenuItem<R>),
    Check(CheckMenuItem<R>),
}

impl<R: Runtime> NativeItem for Handle<R> {
    fn show(&self, state: &MenuItemState) -> tauri::Result<()> {
        match (self, state) {
            (Self::Command(item), MenuItemState::Command { title, enabled }) => {
                item.set_text(title)?;
                item.set_enabled(*enabled)
            }
            (
                Self::Check(item),
                MenuItemState::Check {
                    title,
                    enabled,
                    checked,
                },
            ) => {
                item.set_text(title)?;
                item.set_enabled(*enabled)?;
                item.set_checked(*checked)
            }
            // `MenuItems::apply` keeps each item's kind.
            _ => Ok(()),
        }
    }
}

/// A native item as built: the dashboard's key, the id the system reports it by, its handle.
pub(crate) struct BuiltItem<H> {
    pub key: MenuItemKey,
    pub id: MenuId,
    pub handle: H,
}

/// The id of item `key` in the menu of `generation`: unique across menus, so a click is looked
/// up, never read back from it.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn item_id(generation: u32, key: MenuItemKey) -> MenuId {
    MenuId::new(format!("app-menu/{generation}/{key}"))
}

struct Installed<H> {
    generation: u32,
    items: MenuItems,
    handles: HashMap<MenuItemKey, H>,
    ids: HashMap<MenuId, MenuItemKey>,
}

#[derive(Default)]
struct Inner<H> {
    last_generation: u32,
    installed: Option<Installed<H>>,
}

/// The app's menu as the dashboard installed it, if it is: one per app.
pub(crate) struct Menus<H> {
    inner: Mutex<Inner<H>>,
}

impl<H> Default for Menus<H> {
    fn default() -> Self {
        Self {
            inner: Mutex::new(Inner {
                last_generation: 0,
                installed: None,
            }),
        }
    }
}

/// The menus of the app's runtime.
pub(crate) type AppMenu<R> = Menus<Handle<R>>;

impl<H: NativeItem> Menus<H> {
    /// Installs a menu of `items` under a new generation through `native`, which builds it with
    /// the ids `item_id` gives and makes it the app's menu. If `native` fails, the menu before
    /// stays installed, and so does its generation.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub(crate) fn install(
        &self,
        items: MenuItems,
        native: impl FnOnce(u32) -> anyhow::Result<Vec<BuiltItem<H>>>,
    ) -> Result<u32, MenuError> {
        // Menus are built on the main thread, which holds no other lock meanwhile.
        let mut inner = self.lock();
        let generation = inner.last_generation + 1;
        let built = native(generation).map_err(MenuError::Native)?;
        inner.last_generation = generation;
        let mut handles = HashMap::new();
        let mut ids = HashMap::new();
        for item in built {
            ids.insert(item.id, item.key);
            handles.insert(item.key, item.handle);
        }
        inner.installed = Some(Installed {
            generation,
            items,
            handles,
            ids,
        });
        Ok(generation)
    }

    /// Shows `updates` in the menu of `generation`, if it is still the app's menu: all of them,
    /// or none if one does not fit. A native call that fails partway leaves the items before it
    /// changed (`Native`); the next update sends the states again.
    pub(crate) fn update(
        &self,
        generation: u32,
        updates: &[MenuItemUpdate],
    ) -> Result<(), MenuError> {
        let mut inner = self.lock();
        let installed = inner
            .installed
            .as_mut()
            .filter(|installed| installed.generation == generation)
            .ok_or(MenuError::Stale { generation })?;
        installed.items.apply(updates)?;
        for update in updates {
            if let Some(handle) = installed.handles.get(&update.key) {
                handle
                    .show(&update.state)
                    .map_err(|err| MenuError::Native(err.into()))?;
            }
        }
        Ok(())
    }

    /// The command a chosen item `id` stands for; none for a system item or one of a menu that
    /// is no longer installed.
    pub(crate) fn chosen(&self, id: &MenuId) -> Option<MenuCommand> {
        let inner = self.lock();
        let installed = inner.installed.as_ref()?;
        installed.ids.get(id).map(|&key| MenuCommand {
            generation: installed.generation,
            key,
        })
    }

    /// The page that installed the menu is gone: no update or click of it counts any more.
    pub(crate) fn retire(&self) {
        self.lock().installed = None;
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner<H>> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Makes `spec` the app's menu: checked first, then built and installed where the system has a
/// menu bar (macOS); elsewhere nothing is installed and the dashboard keeps its keys.
#[tauri::command]
pub fn install_menu<R: Runtime>(
    app: AppHandle<R>,
    menus: State<'_, AppMenu<R>>,
    spec: MenuSpec,
) -> Result<MenuInstall, BackendError> {
    let items = MenuItems::of(&spec).map_err(BackendError::from)?;
    imp::install(&app, &menus, &spec, items).map_err(BackendError::from)
}

/// Changes what the items of the menu installed as `generation` show.
#[tauri::command]
pub fn update_menu<R: Runtime>(
    app: AppHandle<R>,
    generation: u32,
    items: Vec<MenuItemUpdate>,
) -> Result<(), BackendError> {
    app.state::<AppMenu<R>>()
        .update(generation, &items)
        .map_err(BackendError::from)
}

/// Reports a chosen item of the app's menu to the dashboard; system items act by themselves.
pub(crate) fn chosen<R: Runtime>(app: &AppHandle<R>, id: &MenuId) {
    let Some(command) = app
        .try_state::<AppMenu<R>>()
        .and_then(|menus| menus.chosen(id))
    else {
        return;
    };
    if let Err(err) = app.emit_to(MAIN_WINDOW, MENU_COMMAND_EVENT, command) {
        diagnostics::warn(
            LOG_SCOPE,
            format!("The menu could not tell the window what was chosen: {err}"),
        );
    }
}

/// The dashboard loads (again): the menu it installed is gone with the page, so the system's
/// default menu comes back until the new page installs its own, and no key is taken by an
/// accelerator the new page does not know of.
pub(crate) fn page_loading<R: Runtime>(app: &AppHandle<R>) {
    let Some(menus) = app.try_state::<AppMenu<R>>() else {
        return;
    };
    menus.retire();
    imp::restore_default(app);
}

#[cfg(not(target_os = "macos"))]
use self::fallback as imp;
#[cfg(target_os = "macos")]
use self::macos as imp;

/// No menu bar of the app's own: nothing is installed, and the dashboard keeps its keys.
#[cfg(not(target_os = "macos"))]
mod fallback {
    use activity_log_core::{
        menu::{MenuError, MenuItems},
        model::{MenuInstall, MenuSpec},
    };
    use tauri::{AppHandle, Runtime};

    use super::AppMenu;

    pub(super) fn install<R: Runtime>(
        _app: &AppHandle<R>,
        _menus: &AppMenu<R>,
        _spec: &MenuSpec,
        _items: MenuItems,
    ) -> Result<MenuInstall, MenuError> {
        Ok(MenuInstall::NotOffered)
    }

    pub(super) fn restore_default<R: Runtime>(_app: &AppHandle<R>) {}
}
