use futures::future::BoxFuture;

use crate::{
    cache::CachePolicy,
    model::{ActivityEntry, CollectOptions, Source, SupportedOs},
    provider::{
        provider_check_result::ProviderCheckResult,
        provider_config::{ConfigBinding, ProviderConfigKey},
        provider_context::ProviderContext,
    },
};

pub trait ProviderAdapter: Send + Sync {
    fn id(&self) -> Source;
    fn supported_os(&self) -> &'static [SupportedOs];

    fn config_binding(&self) -> Option<ConfigBinding> {
        None
    }

    fn config_requirements(&self) -> &'static [ProviderConfigKey] {
        match self.config_binding() {
            Some(binding) => binding.requirements,
            None => &[],
        }
    }

    /// Opt in to the per-day disk cache; meant for providers that call remote APIs.
    fn cache_policy(&self) -> Option<CachePolicy> {
        None
    }

    fn startup_check(
        &self,
        current_os: SupportedOs,
        context: ProviderContext,
    ) -> BoxFuture<'_, ProviderCheckResult>;
    fn collect(
        &self,
        current_os: SupportedOs,
        options: CollectOptions,
        context: ProviderContext,
    ) -> BoxFuture<'_, anyhow::Result<Vec<ActivityEntry>>>;
}
