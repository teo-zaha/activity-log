use futures::future::BoxFuture;

use crate::{
    collect::system::collect_system,
    model::{CollectOptions, Source, SupportedOs},
    provider::{
        provider_adapter::ProviderAdapter, provider_check_result::ProviderCheckResult,
        provider_collection::ProviderCollection, provider_context::ProviderContext,
    },
};

pub mod windows;

pub struct SystemProvider;

impl ProviderAdapter for SystemProvider {
    fn id(&self) -> Source {
        Source::System
    }

    fn supported_os(&self) -> &'static [SupportedOs] {
        &[SupportedOs::Windows]
    }

    fn startup_check(&self, _context: ProviderContext) -> BoxFuture<'_, ProviderCheckResult> {
        Box::pin(async move {
            let result = tokio::task::spawn_blocking(windows::check_system_support).await;
            match result {
                Ok(windows::SystemSupport::Available) => ProviderCheckResult::available(),
                Ok(windows::SystemSupport::Degraded(reason)) => {
                    ProviderCheckResult::degraded(reason)
                }
                Ok(windows::SystemSupport::Unavailable(reason)) => {
                    ProviderCheckResult::unavailable(reason)
                }
                Err(_) => ProviderCheckResult::unavailable("Windows event log unavailable"),
            }
        })
    }

    fn collect(
        &self,
        options: CollectOptions,
        _context: ProviderContext,
    ) -> BoxFuture<'_, anyhow::Result<ProviderCollection>> {
        Box::pin(async move {
            let result = tokio::task::spawn_blocking(move || collect_system(&options)).await;
            let entries = match result {
                Ok(entries) => entries?,
                Err(err) => return Err(err.into()),
            };
            Ok(ProviderCollection::System(entries))
        })
    }
}
