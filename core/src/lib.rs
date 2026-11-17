pub mod aggregator;
pub mod cache;
pub mod collect;
pub mod diagnostics;
pub mod model;
pub(crate) mod platform;
pub mod provider;
pub mod runtime;
pub mod settings;
pub mod ts;
pub mod utils;

#[cfg(test)]
mod tests;

use std::{collections::BTreeMap, future::Future, sync::Arc};

use aggregator::collect_activity_from_results;
use futures::{FutureExt, StreamExt, future::BoxFuture, stream::FuturesUnordered};
use model::{ActivityEntry, ActivityLog, CollectOptions, CollectedActivity, ConnectionId, Source};
use provider::{
    ProviderRuntime,
    provider_adapter::{CollectionState, ConnectionEntries, ProviderAdapter},
};

// Keeps one stuck provider from leaving the whole app in the collecting state.
const COLLECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

/// Collects connections whose startup checks are done, such as the CLI's.
pub async fn collect_activity_detailed_with_provider_runtimes(
    options: CollectOptions,
    provider_runtimes: &[ProviderRuntime],
) -> CollectedActivity {
    let connections = provider_runtimes
        .iter()
        .map(|runtime| (runtime.id().clone(), std::future::ready(runtime.clone())))
        .collect();
    collect_progressively(options, connections, |_| {}).await
}

enum Step {
    Checked(usize, Box<ProviderRuntime>),
    Collected(usize, anyhow::Result<Vec<ActivityEntry>>),
}

/// Where one connection stands in a collection.
enum Slot {
    /// Its startup check or its collection still runs.
    Pending,
    /// Its startup check found it unusable.
    Skipped,
    Done(Vec<ActivityEntry>),
    Failed,
}

/// What the usable connections of one kind share during a collection.
struct Kind {
    adapter: Arc<dyn ProviderAdapter>,
    state: Option<CollectionState>,
    /// The kind's finished connections after `finish_kind`, as of its last change.
    finished: Vec<ConnectionEntries>,
}

/// Collects every connection as soon as its startup check (the future next to its id) ends, so
/// a slow check or a slow source holds back only itself. `on_progress` gets the log of what is
/// there each time a connection settles while others are pending; the complete result is
/// returned.
pub async fn collect_progressively<'a, Check>(
    options: CollectOptions,
    connections: Vec<(ConnectionId, Check)>,
    on_progress: impl Fn(ActivityLog),
) -> CollectedActivity
where
    Check: Future<Output = ProviderRuntime> + Send + 'a,
{
    diagnostics::info(
        "runtime",
        format!(
            "Collecting {} to {}",
            options.start.to_rfc3339(),
            options.end.to_rfc3339()
        ),
    );
    let (ids, checks): (Vec<_>, Vec<_>) = connections.into_iter().unzip();
    let mut slots = ids.iter().map(|_| Slot::Pending).collect::<Vec<_>>();
    let mut kinds = BTreeMap::<Source, Kind>::new();
    let mut steps = checks
        .into_iter()
        .enumerate()
        .map(|(index, check)| {
            check
                .map(move |runtime| Step::Checked(index, Box::new(runtime)))
                .boxed()
        })
        .collect::<FuturesUnordered<BoxFuture<'a, Step>>>();

    while let Some(step) = steps.next().await {
        let index = match step {
            Step::Checked(index, runtime) if runtime.usable() => {
                // Collectors can assume they are called only for supported and configured
                // connections.
                let kind = kinds
                    .entry(ids[index].source.clone())
                    .or_insert_with(|| Kind {
                        adapter: runtime.adapter().clone(),
                        state: runtime.adapter().collection_state(),
                        finished: Vec::new(),
                    });
                steps
                    .push(collect_connection(index, *runtime, options, kind.state.clone()).boxed());
                continue;
            }
            Step::Checked(index, _) => {
                slots[index] = Slot::Skipped;
                index
            }
            Step::Collected(index, Ok(mut entries)) => {
                for entry in &mut entries {
                    entry.set_connection(&ids[index].name);
                }
                slots[index] = Slot::Done(entries);
                index
            }
            Step::Collected(index, Err(_)) => {
                slots[index] = Slot::Failed;
                index
            }
        };
        finish_kind(&ids[index].source, &ids, &slots, &mut kinds);
        if slots.iter().any(|slot| matches!(slot, Slot::Pending)) {
            on_progress(collected_so_far(options, &ids, &slots, &kinds).log);
        }
    }

    collected_so_far(options, &ids, &slots, &kinds)
}

async fn collect_connection(
    index: usize,
    runtime: ProviderRuntime,
    options: CollectOptions,
    state: Option<CollectionState>,
) -> Step {
    let started = std::time::Instant::now();
    let result = tokio::time::timeout(COLLECT_TIMEOUT, runtime.collect(options, state))
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
            runtime.id().to_string(),
            format!("Collected {} entries in {elapsed} ms", entries.len()),
        ),
        Err(err) => diagnostics::error(
            runtime.id().to_string(),
            format!("Collection failed after {elapsed} ms: {err:#}"),
        ),
    }
    Step::Collected(index, result)
}

/// Hands the finished connections of `source` to their kind's `finish_kind` step, in connection
/// order. Called once per connection that settles, so the pass is complete exactly once: when
/// the kind's last connection settles.
fn finish_kind(
    source: &Source,
    ids: &[ConnectionId],
    slots: &[Slot],
    kinds: &mut BTreeMap<Source, Kind>,
) {
    let Some(kind) = kinds.get_mut(source) else {
        return;
    };
    let of_kind = || ids.iter().zip(slots).filter(|(id, _)| id.source == *source);
    let complete = !of_kind().any(|(_, slot)| matches!(slot, Slot::Pending));
    let done = of_kind()
        .filter_map(|(id, slot)| match slot {
            Slot::Done(entries) => Some((id.clone(), entries.clone())),
            _ => None,
        })
        .collect();
    kind.finished = kind
        .adapter
        .finish_kind(kind.state.as_ref(), done, complete);
}

fn collected_so_far(
    options: CollectOptions,
    ids: &[ConnectionId],
    slots: &[Slot],
    kinds: &BTreeMap<Source, Kind>,
) -> CollectedActivity {
    let mut results = Vec::new();
    let mut pending = Vec::new();
    for (id, slot) in ids.iter().zip(slots) {
        match slot {
            Slot::Pending => pending.push(id.clone()),
            Slot::Skipped => {}
            // A failed connection is still listed in the summary, with no entries.
            Slot::Failed => results.push((id.clone(), Vec::new())),
            Slot::Done(_) => {
                let entries = kinds
                    .get(&id.source)
                    .and_then(|kind| kind.finished.iter().find(|(other, _)| other == id))
                    .map(|(_, entries)| entries.clone())
                    .unwrap_or_default();
                results.push((id.clone(), entries));
            }
        }
    }
    collect_activity_from_results(options, results, pending)
}
