use std::{env, ffi::OsString, path::PathBuf};

use anyhow::Result;

use crate::provider::provider_config::{
    ConfigBinding, ProviderConfig, ProviderConfigKey, ProviderConfigResolve,
};
use crate::{model::SupportedOs, provider::current_os};

/// Local repositories, or folders to search for them. Unset means the home folder.
pub const GIT_DIRS: ProviderConfigKey = ProviderConfigKey::optional_string("GIT_DIRS").list_of();
#[cfg(target_os = "windows")]
const CONFIG_KEYS: &[ProviderConfigKey] = &[GIT_DIRS, super::windows::WSL_GIT_DIRS];
#[cfg(not(target_os = "windows"))]
const CONFIG_KEYS: &[ProviderConfigKey] = &[GIT_DIRS];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitConfig {
    /// Local repositories or search folders; `None` means [`default_git_root`].
    pub dirs: Option<Vec<PathBuf>>,
    /// Repositories inside WSL; Windows only.
    #[cfg(any(target_os = "windows", test))]
    pub wsl: super::windows::WslConfig,
}

impl GitConfig {
    pub fn binding() -> ConfigBinding {
        ConfigBinding::of::<Self>()
    }

    /// Exactly these local folders, and nothing in WSL.
    pub fn local_only(dirs: Vec<PathBuf>) -> Self {
        Self {
            dirs: Some(dirs),
            #[cfg(any(target_os = "windows", test))]
            wsl: super::windows::WslConfig::none(),
        }
    }
}

impl ProviderConfigResolve for GitConfig {
    fn requirements() -> &'static [ProviderConfigKey] {
        CONFIG_KEYS
    }

    fn from_config(config: &ProviderConfig) -> Result<Self> {
        Ok(Self {
            dirs: config
                .list(GIT_DIRS)
                .map(|dirs| dirs.into_iter().map(PathBuf::from).collect()),
            #[cfg(any(target_os = "windows", test))]
            wsl: super::windows::WslConfig::from_config(config)?,
        })
    }
}

pub fn default_git_root() -> Option<PathBuf> {
    default_git_root_with_lookup(current_os(), |key| env::var_os(key))
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
