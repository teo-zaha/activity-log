mod config;
#[cfg(any(target_os = "windows", test))]
pub mod windows;

pub use config::*;

use crate::{
    collect::git::{collect_git, open_repo_strict, resolve_git_repos},
    model::{IntoActivityEntries, Source, SupportedOs},
    provider::provider_check_result::ProviderCheckResult,
};

#[cfg(target_os = "windows")]
use self::windows as windows_impl;
#[cfg(not(target_os = "windows"))]
mod windows_impl {
    use crate::provider::{git::GitConfig, provider_check_result::ProviderCheckResult};

    /// WSL needs Windows; elsewhere only the local repositories exist.
    pub(super) fn check_git_ready(config: &GitConfig) -> ProviderCheckResult {
        super::local_check_result(config)
    }
}

crate::provider! {
    GitProvider(Source::Git, &[SupportedOs::Windows, SupportedOs::Macos, SupportedOs::Linux], GitConfig) {
        async fn startup_check(&self, current_os, context) {
            let config = match context.config::<GitConfig>() {
                Ok(c) => c,
                Err(err) => {
                    return ProviderCheckResult::unavailable(format!("Git config is invalid: {err:#}"));
                }
            };

            let config = config.clone();
            tokio::task::spawn_blocking(move || match current_os {
                SupportedOs::Windows => windows_impl::check_git_ready(&config),
                SupportedOs::Macos | SupportedOs::Linux => local_check_result(&config),
            })
            .await
            .unwrap_or_else(|err| {
                ProviderCheckResult::unavailable(format!("Git startup check failed: {err}"))
            })
        }

        async fn collect(&self, current_os, options, context) {
            let config = context.config::<GitConfig>()?.clone();
            let entries = collect_git(&options, &config, current_os).await?;
            Ok(entries.into_activity_entries())
        }
    }
}

/// The basic check: one local repository opens.
fn local_check_result(config: &GitConfig) -> ProviderCheckResult {
    match check_local_repos(config) {
        Ok(()) => ProviderCheckResult::available(),
        Err(reason) => ProviderCheckResult::unavailable(reason),
    }
}

pub(crate) fn check_local_repos(config: &GitConfig) -> Result<(), String> {
    let repos = resolve_git_repos(config).map_err(|err| err.to_string())?;
    if repos.is_empty() {
        return Err("No repositories found".to_string());
    }

    let mut last_error = None;
    for repo in repos {
        match open_repo_strict(&repo) {
            Ok(_) => return Ok(()),
            Err(err) => last_error = Some(format!("{}: {err}", repo.display())),
        }
    }

    Err(match last_error {
        Some(reason) => format!("No readable repositories found: {reason}"),
        None => "No readable repositories found".to_string(),
    })
}
