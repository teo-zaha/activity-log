pub mod aggregator;
pub mod cache;
pub mod collect;
pub mod model;
pub mod provider;
pub mod runtime;
pub mod ts;
pub mod utils;

use aggregator::collect_activity_from_results;
use anyhow::Result;
use model::{ActivityLog, CollectOptions, CollectedActivity};
use provider::{ProviderRuntime, inspect_provider_runtimes};

pub async fn collect_activity(options: CollectOptions) -> Result<ActivityLog> {
    let provider_runtimes = inspect_provider_runtimes().await;
    Ok(
        collect_activity_detailed_with_provider_runtimes(options, &provider_runtimes)
            .await?
            .log,
    )
}

pub async fn collect_activity_detailed(options: CollectOptions) -> Result<CollectedActivity> {
    let provider_runtimes = inspect_provider_runtimes().await;
    collect_activity_detailed_with_provider_runtimes(options, &provider_runtimes).await
}

pub async fn collect_activity_with_provider_runtimes(
    options: CollectOptions,
    provider_runtimes: &[ProviderRuntime],
) -> Result<ActivityLog> {
    Ok(
        collect_activity_detailed_with_provider_runtimes(options, provider_runtimes)
            .await?
            .log,
    )
}

pub async fn collect_activity_detailed_with_provider_runtimes(
    options: CollectOptions,
    provider_runtimes: &[ProviderRuntime],
) -> Result<CollectedActivity> {
    let results = futures::future::join_all(
        // Providers are filtered at startup/runtime level, so collectors can assume they were
        // called only for supported and configured providers.
        provider_runtimes
            .iter()
            .filter(|runtime| runtime.usable())
            .map(|runtime| {
                let options = options.clone();
                async move {
                    let id = runtime.id();
                    let result = runtime.collect(options).await;
                    (id, result)
                }
            }),
    )
    .await;

    Ok(collect_activity_from_results(options, results))
}
