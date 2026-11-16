pub mod aggregator;
pub mod cache;
pub mod collect;
pub mod diagnostics;
pub mod model;
pub(crate) mod platform;
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

// Keeps one stuck provider from leaving the whole app in the collecting state.
const COLLECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

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
    diagnostics::info(
        "runtime",
        format!(
            "Collecting {} to {}",
            options.start.to_rfc3339(),
            options.end.to_rfc3339()
        ),
    );
    let results = futures::future::join_all(
        // Providers are filtered at startup/runtime level, so collectors can assume they were
        // called only for supported and configured providers.
        provider_runtimes
            .iter()
            .filter(|runtime| runtime.usable())
            .map(|runtime| async move {
                let id = runtime.id();
                let started = std::time::Instant::now();
                let result = tokio::time::timeout(COLLECT_TIMEOUT, runtime.collect(options))
                    .await
                    .unwrap_or_else(|_| {
                        Err(anyhow::anyhow!(
                            "Collection timed out after {}s",
                            COLLECT_TIMEOUT.as_secs()
                        ))
                    });
                let elapsed = started.elapsed().as_millis();
                match &result {
                    Ok(entries) => diagnostics::info(
                        id.as_ref(),
                        format!("Collected {} entries in {elapsed} ms", entries.len()),
                    ),
                    Err(err) => diagnostics::error(
                        id.as_ref(),
                        format!("Collection failed after {elapsed} ms: {err:#}"),
                    ),
                }
                (id, result)
            }),
    )
    .await;

    Ok(collect_activity_from_results(options, results))
}
