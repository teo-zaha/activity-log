//! Git inside WSL distributions: which distributions exist and how their folders are written.

#[cfg(test)]
mod tests;

/// Tool-managed distributions hold no user repositories.
const NON_USER_WSL_PREFIXES: &[&str] = &["docker-desktop", "rancher-desktop", "podman-machine"];

/// The user's WSL distributions, read from the registry without starting WSL.
#[cfg(target_os = "windows")]
pub fn default_wsl_distros() -> Vec<String> {
    user_wsl_distros(crate::platform::wsl_distribution_names())
}

/// Ready once the distribution has a repository under the connection's folders.
#[cfg(target_os = "windows")]
pub(super) fn check_wsl_ready(
    distro: &str,
    config: &super::GitConfig,
) -> crate::provider::provider_check_result::ProviderCheckResult {
    use crate::{
        collect::git::{WslTarget, windows::has_wsl_repos},
        provider::provider_check_result::ProviderCheckResult,
    };

    let target = match config.wsl_dirs() {
        Ok(dirs) => WslTarget {
            distro: distro.to_string(),
            dirs,
        },
        Err(err) => return ProviderCheckResult::unavailable(err.to_string()),
    };
    match has_wsl_repos(&target) {
        Ok(false) => {
            ProviderCheckResult::unavailable(format!("No repositories found in WSL {distro}"))
        }
        Ok(true) => ProviderCheckResult::available(),
        Err(reason) => ProviderCheckResult::unavailable(format!("WSL {distro}: {reason}")),
    }
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
