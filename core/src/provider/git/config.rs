use std::{env, ffi::OsString, path::PathBuf};

use anyhow::Result;

use crate::provider::provider_config::{
    ConfigBinding, ProviderConfig, ProviderConfigKey, ProviderConfigResolve,
};
use crate::{model::SupportedOs, provider::current_os};

pub const GIT_DIRS: ProviderConfigKey = ProviderConfigKey::optional_string("GIT_DIRS");
const CONFIG_KEYS: &[ProviderConfigKey] = &[GIT_DIRS];

#[derive(Debug, Clone)]
pub struct GitConfig {
    pub dirs: Option<Vec<PathBuf>>,
}

impl GitConfig {
    pub fn binding() -> ConfigBinding {
        ConfigBinding::of::<Self>()
    }
}

impl ProviderConfigResolve for GitConfig {
    fn requirements() -> &'static [ProviderConfigKey] {
        CONFIG_KEYS
    }

    fn from_config(config: &ProviderConfig) -> Result<Self> {
        let dirs = config.get(GIT_DIRS).and_then(|value| {
            let paths: Vec<PathBuf> = value
                .split(',')
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
                .collect();
            (!paths.is_empty()).then_some(paths)
        });
        Ok(Self { dirs })
    }
}

pub fn default_git_root() -> Option<PathBuf> {
    default_git_root_with_lookup(current_os(), |key| env::var_os(key))
}

pub fn default_git_dirs_value() -> Option<String> {
    default_git_root().map(|path| path.display().to_string())
}

pub(crate) fn default_git_root_with_lookup<F>(os: SupportedOs, lookup: F) -> Option<PathBuf>
where
    F: Fn(&str) -> Option<OsString>,
{
    match os {
        SupportedOs::Windows => lookup("USERPROFILE")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                let drive = lookup("HOMEDRIVE")?;
                let path = lookup("HOMEPATH")?;
                if drive.is_empty() || path.is_empty() {
                    return None;
                }

                let mut combined = PathBuf::from(drive);
                combined.push(path);
                Some(combined)
            })
            .or_else(|| {
                lookup("HOME")
                    .filter(|value| !value.is_empty())
                    .map(PathBuf::from)
            }),
        SupportedOs::Macos | SupportedOs::Linux => lookup("HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from),
    }
}
