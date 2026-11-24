use std::path::PathBuf;

pub(super) fn settings_path() -> Option<PathBuf> {
    crate::platform::dirs::config_home().map(|home| home.join("Figma").join("settings.json"))
}
