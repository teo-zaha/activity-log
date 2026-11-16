//! Windows-only Git settings: repositories inside WSL distributions.

use std::collections::BTreeMap;

use anyhow::{Result, anyhow};

use crate::provider::provider_config::{ProviderConfig, ProviderConfigKey};

#[cfg(test)]
mod tests;

/// Folders per WSL distribution, read like `GIT_DIRS`: a repository, or a folder searched for
/// them. Unset means the home folder of every user distribution.
pub const WSL_GIT_DIRS: ProviderConfigKey =
    ProviderConfigKey::optional_string("WSL_GIT_DIRS").map_of();

/// A distribution's home folder, as a WSL folder or the start of one (`~/work`).
pub const WSL_HOME: &str = "~";

/// Tool-managed distributions hold no user repositories.
const NON_USER_WSL_PREFIXES: &[&str] = &["docker-desktop", "rancher-desktop", "podman-machine"];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WslConfig {
    /// Folders by distribution; `None` means [`default_wsl_dirs`], an empty map nothing.
    pub dirs: Option<BTreeMap<String, Vec<String>>>,
}

impl WslConfig {
    /// Searches nothing in WSL.
    pub fn none() -> Self {
        Self {
            dirs: Some(BTreeMap::new()),
        }
    }

    pub(crate) fn from_config(config: &ProviderConfig) -> Result<Self> {
        let Some(map) = config.map(WSL_GIT_DIRS) else {
            return Ok(Self { dirs: None });
        };
        let dirs = map
            .into_iter()
            .map(|(distro, dirs)| {
                let dirs = dirs
                    .iter()
                    .map(|dir| parse_wsl_dir(&distro, dir))
                    .collect::<Result<_>>()?;
                Ok((distro, dirs))
            })
            .collect::<Result<_>>()?;
        Ok(Self { dirs: Some(dirs) })
    }
}

/// An absolute Linux path, `~` or `~/path`, without a trailing slash.
pub(crate) fn parse_wsl_dir(distro: &str, dir: &str) -> Result<String> {
    if dir == WSL_HOME || dir.starts_with("~/") || dir.starts_with('/') {
        let trimmed = dir.trim_end_matches('/');
        Ok(if trimmed.is_empty() { "/" } else { trimmed }.to_string())
    } else {
        Err(anyhow!(
            "Invalid WSL folder \"{dir}\" in {distro}: expected an absolute Linux path or ~"
        ))
    }
}

/// Available once a local repository opens or a WSL repository exists. Local folders are
/// checked first, so WSL is only started when they hold nothing.
#[cfg(target_os = "windows")]
pub(super) fn check_git_ready(
    config: &super::GitConfig,
) -> crate::provider::provider_check_result::ProviderCheckResult {
    use crate::provider::provider_check_result::ProviderCheckResult;

    let Err(local) = super::check_local_repos(config) else {
        return ProviderCheckResult::available();
    };
    match crate::collect::git::windows::count_wsl_repos(config) {
        Ok(0) => ProviderCheckResult::unavailable(local),
        Ok(_) => ProviderCheckResult::available(),
        Err(reason) => ProviderCheckResult::unavailable(format!("{local}; WSL: {reason}")),
    }
}

/// The user's WSL distributions, read from the registry without starting WSL.
#[cfg(target_os = "windows")]
pub fn default_wsl_distros() -> Vec<String> {
    user_wsl_distros(crate::platform::wsl_distribution_names())
}

/// The home folder of every user distribution, used while `WSL_GIT_DIRS` is unset.
#[cfg(target_os = "windows")]
pub fn default_wsl_dirs() -> BTreeMap<String, Vec<String>> {
    home_dirs(default_wsl_distros())
}

pub(crate) fn home_dirs(distros: Vec<String>) -> BTreeMap<String, Vec<String>> {
    distros
        .into_iter()
        .map(|distro| (distro, vec![WSL_HOME.to_string()]))
        .collect()
}

pub(crate) fn user_wsl_distros(names: Vec<String>) -> Vec<String> {
    names
        .into_iter()
        .filter(|name| {
            let lower = name.to_ascii_lowercase();
            !name.trim().is_empty()
                && !NON_USER_WSL_PREFIXES
                    .iter()
                    .any(|prefix| lower.starts_with(prefix))
        })
        .collect()
}
