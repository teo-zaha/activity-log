//! The names the desktop app's commands and events go by, in one place: the desktop registers
//! and emits them, and `generate-ts` hands them to the dashboard (`DESKTOP_COMMANDS`,
//! `DESKTOP_EVENTS`). Tauri names a command by its function, so a desktop test checks that every
//! name here is one it registered. The menu's are in `crate::menu`.

/// Every desktop command, by the key the dashboard calls it with and its name.
pub const DESKTOP_COMMANDS: &[(&str, &str)] = &[
    ("getRuntimeState", "get_runtime_state"),
    ("collect", "collect"),
    ("getProviderConfig", "get_provider_config"),
    ("checkProviderConfig", "check_provider_config"),
    ("saveProviderConfig", "save_provider_config"),
    ("addConnection", "add_connection"),
    ("removeConnection", "remove_connection"),
    ("recheckConnection", "recheck_connection"),
    ("saveExport", "save_export"),
    ("openUrl", "open_url"),
    ("openLocalFix", "open_local_fix"),
    ("getLogs", "get_logs"),
    ("clearLogs", "clear_logs"),
    ("takeLaunchDeepLink", "take_launch_deep_link"),
    ("takeLegacyImportReport", "take_legacy_import_report"),
    ("importLegacySettings", "import_legacy_settings"),
    ("attachMaximizeButton", "attach_maximize_button"),
];

/// A link from the OS once the dashboard listens; the payload is the URL.
pub const DEEP_LINK_EVENT: &str = "deep-link";
/// The pointer over the maximize button's snap-layout window (`MaximizeButtonState`).
pub const MAXIMIZE_BUTTON_EVENT: &str = "maximize-button";

/// Every desktop event but the host's state (`RuntimeMessage::EVENTS`) and the menu's, by the
/// key the dashboard listens with and its name.
pub const DESKTOP_EVENTS: &[(&str, &str)] = &[
    ("deepLink", DEEP_LINK_EVENT),
    ("maximizeButton", MAXIMIZE_BUTTON_EVENT),
];
