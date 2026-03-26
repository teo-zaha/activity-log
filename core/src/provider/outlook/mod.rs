use futures::future::BoxFuture;

use crate::{
    collect::outlook::collect_outlook,
    model::{CollectOptions, Source, SupportedOs},
    provider::{
        provider_adapter::ProviderAdapter, provider_check_result::ProviderCheckResult,
        provider_collection::ProviderCollection, provider_context::ProviderContext,
    },
};

pub mod macos;
pub mod windows;

pub struct OutlookProvider;

impl ProviderAdapter for OutlookProvider {
    fn id(&self) -> Source {
        Source::Outlook
    }

    fn supported_os(&self) -> &'static [SupportedOs] {
        &[SupportedOs::Windows, SupportedOs::Macos]
    }

    fn startup_check(
        &self,
        current_os: SupportedOs,
        _context: ProviderContext,
    ) -> BoxFuture<'_, ProviderCheckResult> {
        Box::pin(async move {
            let result = tokio::task::spawn_blocking(move || match current_os {
                SupportedOs::Windows => windows::check_outlook_ready(),
                SupportedOs::Macos => macos::check_outlook_ready(),
                SupportedOs::Linux => Err(anyhow::anyhow!("OS not supported")),
            })
            .await;
            match result {
                Ok(Ok(())) => ProviderCheckResult::available(),
                Ok(Err(err)) => ProviderCheckResult::unavailable(err.to_string()),
                Err(_) => ProviderCheckResult::unavailable("Outlook startup check failed"),
            }
        })
    }

    fn collect(
        &self,
        current_os: SupportedOs,
        options: CollectOptions,
        _context: ProviderContext,
    ) -> BoxFuture<'_, anyhow::Result<ProviderCollection>> {
        Box::pin(async move {
            let entries =
                tokio::task::spawn_blocking(move || collect_outlook(&options, current_os))
                    .await??;
            Ok(ProviderCollection::Outlook(entries))
        })
    }
}
