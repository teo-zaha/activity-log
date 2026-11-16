//! Windows-only Git settings: repositories inside WSL distributions.

use anyhow::{Result, anyhow};

use crate::provider::provider_config::{ProviderConfig, ProviderConfigKey};

#[cfg(test)]
mod tests;

/// WSL distributions whose `$HOME` is searched. Unset means every user distribution.
pub const WSL_DISTROS: ProviderConfigKey =
    ProviderConfigKey::optional_string("WSL_DISTROS").list_of();
/// WSL repositories as `Distro:/absolute/linux/path`, collected without searching.
pub const WSL_GIT_DIRS: ProviderConfigKey =
    ProviderConfigKey::optional_string("WSL_GIT_DIRS").list_of();

/// Tool-managed distributions hold no user repositories.
const NON_USER_WSL_PREFIXES: &[&str] = &["docker-desktop", "rancher-desktop", "podman-machine"];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WslConfig {
    /// Distributions to search; `None` means [`default_wsl_distros`], an empty list none.
    pub distros: Option<Vec<String>>,
    pub repos: Vec<WslRepo>,
}

/// A repository inside a WSL distribution, at a Linux path.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct WslRepo {
    pub distro: String,
    pub path: String,
}

impl WslConfig {
    /// Searches nothing in WSL.
    pub fn none() -> Self {
        Self {
            distros: Some(Vec::new()),
            repos: Vec::new(),
        }
    }

    pub(crate) fn from_config(config: &ProviderConfig) -> Result<Self> {
        Ok(Self {
            distros: config.list(WSL_DISTROS),
            repos: config
                .list(WSL_GIT_DIRS)
                .unwrap_or_default()
                .iter()
                .map(|spec| parse_wsl_repo(spec))
                .collect::<Result<_>>()?,
        })
    }
}

/// `Distro:/absolute/linux/path`; a one-letter "distro" is a Windows drive, not WSL.
pub(crate) fn parse_wsl_repo(spec: &str) -> Result<WslRepo> {
    match spec.split_once(':') {
        Some((distro, path)) if distro.len() > 1 && path.starts_with('/') => Ok(WslRepo {
            distro: distro.to_string(),
            path: path.trim_end_matches('/').to_string(),
        }),
        _ => Err(anyhow!(
            "Invalid WSL repository \"{spec}\": expected Distro:/absolute/linux/path"
        )),
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
