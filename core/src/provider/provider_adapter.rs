use futures::future::BoxFuture;

use crate::{
    model::{CollectOptions, Source, SupportedOs},
    provider::{
        provider_check_result::ProviderCheckResult, provider_collection::ProviderCollection,
        provider_config::ProviderConfigKey, provider_context::ProviderContext,
    },
};

pub trait ProviderAdapter: Send + Sync {
    fn id(&self) -> Source;
    fn supported_os(&self) -> &'static [SupportedOs];
    fn config_requirements(&self) -> &'static [ProviderConfigKey] {
        &[]
    }
    fn startup_check(&self, context: ProviderContext) -> BoxFuture<'_, ProviderCheckResult>;
    fn collect(
        &self,
        options: CollectOptions,
        context: ProviderContext,
    ) -> BoxFuture<'_, anyhow::Result<ProviderCollection>>;
}
