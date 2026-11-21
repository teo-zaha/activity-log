use std::{env, ffi::OsString, path::PathBuf};

use anyhow::{Result, anyhow};

use crate::provider::provider_config::{
    ConfigBinding, ProviderConfig, ProviderConfigKey, ProviderConfigResolve,
    ProviderConfigValueType,
};
use crate::{model::SupportedOs, provider::current_os};

/// Repositories, or folders to search for them. Unset means the home folder: this computer's,
/// or the distribution's (`~`) for a WSL connection.
pub const GIT_DIRS: ProviderConfigKey =
    ProviderConfigKey::optional("GIT_DIRS", ProviderConfigValueType::String)
        .labelled("Git Dirs")
        .list_of();
const CONFIG_KEYS: &[ProviderConfigKey] = &[GIT_DIRS];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitConfig {
    /// Repositories or search folders; `None` means the home folder.
    pub dirs: Option<Vec<PathBuf>>,
}

impl GitConfig {
    pub fn binding() -> ConfigBinding {
        ConfigBinding::of::<Self>()
    }

    /// Exactly these folders.
    pub fn with_dirs(dirs: Vec<PathBuf>) -> Self {
        Self { dirs: Some(dirs) }
    }

    /// The folders as Linux paths inside a WSL distribution, `~` while unset.
    pub fn wsl_dirs(&self) -> Result<Vec<String>> {
        let Some(dirs) = &self.dirs else {
            return Ok(vec![WSL_HOME.to_string()]);
        };
        dirs.iter()
            .map(|dir| parse_wsl_dir(&dir.to_string_lossy()))
            .collect()
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
        })
    }
}

pub fn wsl_label(distro: &str) -> String {
    format!("WSL · {distro}")
}

/// A distribution's home folder, as a WSL folder or the start of one (`~/work`).
pub const WSL_HOME: &str = "~";

/// An absolute Linux path, `~` or `~/path`, without a trailing slash.
pub(crate) fn parse_wsl_dir(dir: &str) -> Result<String> {
    let dir = dir.trim();
    if dir == WSL_HOME || dir.starts_with("~/") || dir.starts_with('/') {
        let trimmed = dir.trim_end_matches('/');
        Ok(if trimmed.is_empty() { "/" } else { trimmed }.to_string())
    } else {
        Err(anyhow!(
            "Invalid WSL folder \"{dir}\": expected an absolute Linux path or ~"
        ))
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
