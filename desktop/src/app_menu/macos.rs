//! The native menu bar on macOS, built from `layout`.

use activity_log_core::{
    diagnostics,
    menu::{MenuError, MenuItems},
    model::{MenuInstall, MenuItemState, MenuKey, MenuSpec, MenuSubmenu},
};
use tauri::{
    AppHandle, Runtime,
    menu::{
        AboutMetadata, CheckMenuItem, HELP_SUBMENU_ID, Menu, MenuItem, PredefinedMenuItem, Submenu,
        WINDOW_SUBMENU_ID,
    },
};

use super::{AppMenu, BuiltItem, Handle, Slot, SystemItem, item_id, layout};

pub(super) fn install<R: Runtime>(
    app: &AppHandle<R>,
    menus: &AppMenu<R>,
    spec: &MenuSpec,
    items: MenuItems,
) -> Result<MenuInstall, MenuError> {
    let generation = menus.install(items, |generation| {
        let (menu, built) = build(app, generation, spec)?;
        app.set_menu(menu)?;
        Ok(built)
    })?;
    Ok(MenuInstall::Installed { generation })
}

/// Puts Tauri's default menu back; without one, the system items (Quit, copy and paste) are gone.
pub(super) fn restore_default<R: Runtime>(app: &AppHandle<R>) {
    if let Err(err) = Menu::default(app).and_then(|menu| app.set_menu(menu)) {
        diagnostics::warn(
            super::LOG_SCOPE,
            format!("The default menu could not be restored: {err}"),
        );
    }
}

/// A menu as built, and the app's items in it.
type Built<R> = (Menu<R>, Vec<BuiltItem<Handle<R>>>);

fn build<R: Runtime>(
    app: &AppHandle<R>,
    generation: u32,
    spec: &MenuSpec,
) -> tauri::Result<Built<R>> {
    let menu = Menu::new(app)?;
    let mut built = Vec::new();
    for submenu in layout(spec) {
        let native = match submenu.submenu {
            MenuSubmenu::App => Submenu::new(app, &app.package_info().name, true)?,
            MenuSubmenu::File => Submenu::new(app, "File", true)?,
            MenuSubmenu::Edit => Submenu::new(app, "Edit", true)?,
            MenuSubmenu::View => Submenu::new(app, "View", true)?,
            // The ids by which macOS lists the windows and puts its help search there.
            MenuSubmenu::Window => Submenu::with_id(app, WINDOW_SUBMENU_ID, "Window", true)?,
            MenuSubmenu::Help => Submenu::with_id(app, HELP_SUBMENU_ID, "Help", true)?,
        };
        for slot in submenu.slots {
            match slot {
                Slot::System(item) => native.append(&system_item(app, item)?)?,
                Slot::Separator => native.append(&PredefinedMenuItem::separator(app)?)?,
                Slot::App(item) => {
                    let id = item_id(generation, item.key);
                    let accelerator = item.accelerator.map(MenuKey::accelerator);
                    let handle = match &item.state {
                        MenuItemState::Command { title, enabled } => {
                            let native_item =
                                MenuItem::with_id(app, id.clone(), title, *enabled, accelerator)?;
                            native.append(&native_item)?;
                            Handle::Command(native_item)
                        }
                        MenuItemState::Check {
                            title,
                            enabled,
                            checked,
                        } => {
                            let native_item = CheckMenuItem::with_id(
                                app,
                                id.clone(),
                                title,
                                *enabled,
                                *checked,
                                accelerator,
                            )?;
                            native.append(&native_item)?;
                            Handle::Check(native_item)
                        }
                    };
                    built.push(BuiltItem {
                        key: item.key,
                        id,
                        handle,
                    });
                }
            }
        }
        menu.append(&native)?;
    }
    Ok((menu, built))
}

fn system_item<R: Runtime>(
    app: &AppHandle<R>,
    item: SystemItem,
) -> tauri::Result<PredefinedMenuItem<R>> {
    match item {
        SystemItem::About => {
            let info = app.package_info();
            let bundle = &app.config().bundle;
            let metadata = AboutMetadata {
                name: Some(info.name.clone()),
                version: Some(info.version.to_string()),
                copyright: bundle.copyright.clone(),
                authors: bundle.publisher.clone().map(|publisher| vec![publisher]),
                ..Default::default()
            };
            PredefinedMenuItem::about(app, None, Some(metadata))
        }
        SystemItem::Services => PredefinedMenuItem::services(app, None),
        SystemItem::Hide => PredefinedMenuItem::hide(app, None),
        SystemItem::HideOthers => PredefinedMenuItem::hide_others(app, None),
        SystemItem::ShowAll => PredefinedMenuItem::show_all(app, None),
        SystemItem::Quit => PredefinedMenuItem::quit(app, None),
        SystemItem::CloseWindow => PredefinedMenuItem::close_window(app, None),
        SystemItem::Undo => PredefinedMenuItem::undo(app, None),
        SystemItem::Redo => PredefinedMenuItem::redo(app, None),
        SystemItem::Cut => PredefinedMenuItem::cut(app, None),
        SystemItem::Copy => PredefinedMenuItem::copy(app, None),
        SystemItem::Paste => PredefinedMenuItem::paste(app, None),
        SystemItem::SelectAll => PredefinedMenuItem::select_all(app, None),
        SystemItem::Fullscreen => PredefinedMenuItem::fullscreen(app, None),
        SystemItem::Minimize => PredefinedMenuItem::minimize(app, None),
        SystemItem::Zoom => PredefinedMenuItem::maximize(app, None),
        SystemItem::BringAllToFront => PredefinedMenuItem::bring_all_to_front(app, None),
    }
}
