mod config;

pub use config::*;

use crate::{
    collect::git::{collect_git, open_repo_strict, resolve_git_repos},
    model::{IntoActivityEntries, Source, SupportedOs},
    provider::provider_check_result::ProviderCheckResult,
};

crate::provider! {
    GitProvider(Source::Git, &[SupportedOs::Windows, SupportedOs::Macos, SupportedOs::Linux], GitConfig) {
        async fn startup_check(&self, _current_os, context) {
            let config = match context.config::<GitConfig>() {
                Ok(c) => c,
                Err(_) => {
                    return ProviderCheckResult::unavailable("Git config resolution failed");
                }
            };

            let config = config.clone();
            tokio::task::spawn_blocking(move || check_git_repos(&config))
                .await
                .unwrap_or_else(|err| {
                    ProviderCheckResult::unavailable(format!("Git startup check failed: {err}"))
                })
        }

        async fn collect(&self, _current_os, options, context) {
            let config = context.config::<GitConfig>()?.clone();
            let entries = collect_git(&options, &config).await?;
            Ok(entries.into_activity_entries())
        }
    }
}

fn check_git_repos(config: &GitConfig) -> ProviderCheckResult {
    let repos = match resolve_git_repos(config) {
        Ok(repos) => repos,
        Err(err) => return ProviderCheckResult::unavailable(err.to_string()),
    };
    if repos.is_empty() {
        return ProviderCheckResult::unavailable("No repositories found");
    }

    let mut last_error = None;
    for repo in repos {
        match open_repo_strict(&repo) {
            Ok(_) => return ProviderCheckResult::available(),
            Err(err) => last_error = Some(format!("{}: {err}", repo.display())),
        }
    }

    match last_error {
        Some(reason) => {
            ProviderCheckResult::unavailable(format!("No readable repositories found: {reason}"))
        }
        None => ProviderCheckResult::unavailable("No readable repositories found"),
    }
}
