use futures::future::BoxFuture;

use crate::{
    collect::git::{
        collect_git, discover_home_repos, git_is_available, resolve_git_dirs_from_context,
    },
    model::{CollectOptions, Source, SupportedOs},
    provider::{
        ALL_OS, provider_adapter::ProviderAdapter, provider_check_result::ProviderCheckResult,
        provider_collection::ProviderCollection, provider_config::ProviderConfigKey,
        provider_context::ProviderContext,
    },
};

pub struct GitProvider;

pub const GIT_DIRS: ProviderConfigKey = ProviderConfigKey::optional_string("GIT_DIRS");

const CONFIG_KEYS: &[ProviderConfigKey] = &[GIT_DIRS];

impl ProviderAdapter for GitProvider {
    fn id(&self) -> Source {
        Source::Git
    }

    fn supported_os(&self) -> &'static [SupportedOs] {
        ALL_OS
    }

    fn config_requirements(&self) -> &'static [ProviderConfigKey] {
        CONFIG_KEYS
    }

    fn startup_check(&self, context: ProviderContext) -> BoxFuture<'_, ProviderCheckResult> {
        Box::pin(async move {
            if !git_is_available() {
                return ProviderCheckResult::unavailable("Git not installed");
            }

            let repos = resolve_git_dirs_from_context(&context).unwrap_or_else(discover_home_repos);
            if repos.is_empty() {
                return ProviderCheckResult::unavailable("No repositories found");
            }

            ProviderCheckResult::available()
        })
    }

    fn collect(
        &self,
        options: CollectOptions,
        context: ProviderContext,
    ) -> BoxFuture<'_, anyhow::Result<ProviderCollection>> {
        Box::pin(async move {
            let result = tokio::task::spawn_blocking(move || collect_git(&options, &context)).await;
            let entries = match result {
                Ok(entries) => entries?,
                Err(err) => return Err(err.into()),
            };
            Ok(ProviderCollection::Git(entries))
        })
    }
}
