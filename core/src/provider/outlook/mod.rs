use futures::future::BoxFuture;

use crate::{
    collect::outlook::collect_outlook,
    model::{CollectOptions, Source, SupportedOs},
    provider::{
        provider_adapter::ProviderAdapter, provider_check_result::ProviderCheckResult,
        provider_collection::ProviderCollection, provider_context::ProviderContext,
    },
};

pub mod windows;

pub struct OutlookProvider;

impl ProviderAdapter for OutlookProvider {
    fn id(&self) -> Source {
        Source::Outlook
    }

    fn supported_os(&self) -> &'static [SupportedOs] {
        &[SupportedOs::Windows]
    }

    fn startup_check(&self, _context: ProviderContext) -> BoxFuture<'_, ProviderCheckResult> {
        Box::pin(async move {
            let result = tokio::task::spawn_blocking(windows::check_outlook_installed).await;
            match result {
                Ok(Ok(())) => ProviderCheckResult::available(),
                _ => ProviderCheckResult::unavailable("Outlook not installed"),
            }
        })
    }

    fn collect(
        &self,
        options: CollectOptions,
        _context: ProviderContext,
    ) -> BoxFuture<'_, anyhow::Result<ProviderCollection>> {
        Box::pin(async move {
            let result = tokio::task::spawn_blocking(move || collect_outlook(&options)).await;
            let entries = match result {
                Ok(entries) => entries?,
                Err(err) => return Err(err.into()),
            };
            Ok(ProviderCollection::Outlook(entries))
        })
    }
}
