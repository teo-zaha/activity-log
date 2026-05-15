mod config;

pub use config::*;

use crate::{
    collect::git::{collect_git, git_is_available, resolve_git_repos},
    model::{IntoActivityEntries, Source, SupportedOs},
    provider::provider_check_result::ProviderCheckResult,
};

crate::provider! {
    GitProvider(Source::Git, &[SupportedOs::Windows, SupportedOs::Macos, SupportedOs::Linux], GitConfig) {
        async fn startup_check(&self, _current_os, context) {
            if !git_is_available().await {
                return ProviderCheckResult::unavailable("Git not installed");
            }

            let config = match context.config::<GitConfig>() {
                Ok(c) => c,
                Err(_) => {
                    return ProviderCheckResult::unavailable("Git config resolution failed");
                }
            };

            let repos = match resolve_git_repos(config) {
                Ok(repos) => repos,
                Err(err) => {
                    return ProviderCheckResult::unavailable(err.to_string());
                }
            };
            if repos.is_empty() {
                return ProviderCheckResult::unavailable("No repositories found");
            }

            ProviderCheckResult::available()
        }

        async fn collect(&self, _current_os, options, context) {
            let config = context.config::<GitConfig>()?.clone();
            let entries = collect_git(&options, &config).await?;
            Ok(entries.into_activity_entries())
        }
    }
}
