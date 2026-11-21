pub mod aggregator;
mod backend_error;
pub mod cache;
pub mod collect;
pub mod diagnostics;
pub mod export;
pub mod ipc;
pub mod menu;
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

use aggregator::make_ids_unique;
use futures::{FutureExt, StreamExt, future::BoxFuture, stream::FuturesUnordered};
use model::{ActivityEntry, CollectOptions, CollectedActivity, ConnectionId, Source};
use provider::{
    ProviderRuntime,
    provider_adapter::{CollectionState, ConnectionEntries, ProviderAdapter},
};
use runtime::report::{
    Checked, CollectFailure, CollectionKey, Outcome, Plan, RunReport, Slot as ReportSlot, Stored,
    Superseded,
};

// Keeps one stuck provider from leaving the whole app in the collecting state.
const COLLECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);
/// How long a connection's `annotate` may take: its own deadline is `STACK_BUDGET` (change
/// stacks), so this only stops an adapter that overruns it. Entries are only lent to it, so
/// none is lost when it is stopped.
const ANNOTATE_TIMEOUT: std::time::Duration =
    collect::change_stack::STACK_BUDGET.saturating_add(std::time::Duration::from_secs(2));

/// Collects connections whose startup checks are done, such as the CLI's: the report says how
/// each one ended, `RunReport::project` makes the log.
pub async fn collect_runtimes(
    options: CollectOptions,
    provider_runtimes: &[ProviderRuntime],
) -> RunReport {
    let connections = provider_runtimes
        .iter()
        .map(|runtime| (runtime.id().clone(), std::future::ready(runtime.clone())))
        .collect();
    collect_planned(options, checked_plans(connections), |_| {}).await
}

/// Collects every connection as soon as its startup check (the future next to its id) ends, so
/// a slow check or a slow source holds back only itself. `on_progress` gets the log of what is
/// there each time a connection settles while others are pending; the complete result is
/// returned. For a collection outside a host, which keeps nothing between runs.
pub async fn collect_progressively<Check>(
    options: CollectOptions,
    connections: Vec<(ConnectionId, Check)>,
    on_progress: impl Fn(model::ActivityLog),
) -> CollectedActivity
where
    Check: Future<Output = ProviderRuntime> + Send + 'static,
{
    collect_planned(options, checked_plans(connections), |report| {
        on_progress(report.project(options).log)
    })
    .await
    .project(options)
}

/// A plan per connection that collects it once its check ends, carrying nothing.
fn checked_plans<Check>(connections: Vec<(ConnectionId, Check)>) -> Vec<(ConnectionId, Plan)>
where
    Check: Future<Output = ProviderRuntime> + Send + 'static,
{
    connections
        .into_iter()
        .map(|(id, check)| {
            let check = check
                .map(|runtime| {
                    Ok(Checked {
                        runtime,
                        key: CollectionKey::UNKEYED,
                    })
                })
                .boxed();
            (
                id,
                Plan::Collect {
                    check,
                    carried: None,
                },
            )
        })
        .collect()
}

enum Step {
    Checked(usize, Result<Box<Checked>, Superseded>),
    Collected(usize, Result<Vec<ActivityEntry>, CollectFailure>),
}

/// Where one connection stands in a collection.
enum Slot {
    /// Its startup check or its collection still runs; shows what it carries meanwhile.
    Pending(Option<Stored>),
    /// Its startup check found it unusable, or was replaced.
    Skipped,
    /// Collected under this key, as the source gave them; what shows is its kind's `finished`.
    Done(CollectionKey, Vec<ActivityEntry>),
    Failed(CollectionKey, CollectFailure),
    /// Not collected: what an earlier collection found stays.
    Kept(Outcome),
}

/// What the usable connections of one kind share during a collection.
struct Kind {
    adapter: Arc<dyn ProviderAdapter>,
    state: Option<CollectionState>,
    /// The kind's finished connections after `finish_kind`, as of its last change.
    finished: Vec<ConnectionEntries>,
}

/// Collects each planned connection as soon as its check ends (`Plan::Collect`) or keeps what
/// an earlier collection found for it (`Plan::Keep`). `on_progress` gets the report each time a
/// connection settles while others are pending; the final report is returned. A connection's
/// entries are stamped with the key its check yielded.
pub async fn collect_planned(
    options: CollectOptions,
    connections: Vec<(ConnectionId, Plan)>,
    on_progress: impl Fn(RunReport),
) -> RunReport {
    diagnostics::info(
        "runtime",
        format!(
            "Collecting {} to {}",
            options.start.to_rfc3339(),
            options.end.to_rfc3339()
        ),
    );
    let (ids, plans): (Vec<_>, Vec<_>) = connections.into_iter().unzip();
    let mut slots = Vec::with_capacity(plans.len());
    let mut keys = vec![CollectionKey::UNKEYED; plans.len()];
    let mut steps = FuturesUnordered::<BoxFuture<'static, Step>>::new();
    for (index, plan) in plans.into_iter().enumerate() {
        match plan {
            Plan::Collect { check, carried } => {
                slots.push(Slot::Pending(carried));
                steps.push(
                    check
                        .map(move |checked| Step::Checked(index, checked.map(Box::new)))
                        .boxed(),
                );
            }
            Plan::Keep(outcome) => slots.push(Slot::Kept(outcome)),
        }
    }
    let mut kinds = BTreeMap::<Source, Kind>::new();

    while let Some(step) = steps.next().await {
        let index = match step {
            Step::Checked(index, Ok(checked)) if checked.runtime.usable() => {
                let Checked { runtime, key } = *checked;
                keys[index] = key;
                // Collectors can assume they are called only for supported and configured
                // connections.
                let kind = kinds
                    .entry(ids[index].source.clone())
                    .or_insert_with(|| Kind {
                        adapter: runtime.adapter().clone(),
                        state: runtime.adapter().collection_state(),
                        finished: Vec::new(),
                    });
                steps.push(collect_connection(index, runtime, options, kind.state.clone()).boxed());
                continue;
            }
            Step::Checked(index, _) => {
                slots[index] = Slot::Skipped;
                index
            }
            Step::Collected(index, Ok(entries)) => {
                slots[index] = Slot::Done(keys[index], entries);
                index
            }
            Step::Collected(index, Err(err)) => {
                slots[index] = Slot::Failed(keys[index], err);
                index
            }
        };
        finish_kind(&ids[index].source, &ids, &slots, &mut kinds);
        if slots.iter().any(|slot| matches!(slot, Slot::Pending(_))) {
            on_progress(report(&ids, &slots, &kinds));
        }
    }
    report(&ids, &slots, &kinds)
}

async fn collect_connection(
    index: usize,
    runtime: ProviderRuntime,
    options: CollectOptions,
    state: Option<CollectionState>,
) -> Step {
    let started = std::time::Instant::now();
    let result = match tokio::time::timeout(COLLECT_TIMEOUT, runtime.collect(options, state)).await
    {
        Ok(Ok(entries)) => Ok(entries),
        Ok(Err(err)) => Err(CollectFailure::of(&err)),
        Err(_) => Err(CollectFailure::timed_out(COLLECT_TIMEOUT)),
    };
    let elapsed = started.elapsed().as_millis();
    match &result {
        Ok(entries) => diagnostics::info(
            runtime.id().to_string(),
            format!("Collected {} entries in {elapsed} ms", entries.len()),
        ),
        Err(failure) => diagnostics::error(
            runtime.id().to_string(),
            format!(
                "Collection failed after {elapsed} ms: {}",
                failure.message()
            ),
        ),
    }
    // After the day cache and outside the collection's own timeout: what it adds is true now,
    // never cached, and a slow or failed lookup costs the connection none of its entries.
    let result = match result {
        Ok(mut entries) => {
            annotate(&runtime, &mut entries).await;
            Ok(entries)
        }
        Err(failure) => Err(failure),
    };
    Step::Collected(index, result)
}

/// `runtime.annotate(entries)`, stopped once it overruns [`ANNOTATE_TIMEOUT`].
async fn annotate(runtime: &ProviderRuntime, entries: &mut [ActivityEntry]) {
    if tokio::time::timeout(ANNOTATE_TIMEOUT, runtime.annotate(entries))
        .await
        .is_err()
    {
        diagnostics::warn(
            runtime.id().to_string(),
            format!(
                "Stopped adding current details after {} s; the entries show without them",
                ANNOTATE_TIMEOUT.as_secs()
            ),
        );
    }
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
    let complete = !of_kind().any(|(_, slot)| matches!(slot, Slot::Pending(_)));
    let done = of_kind()
        .filter_map(|(id, slot)| match slot {
            Slot::Done(_, entries) => Some((id.clone(), entries.clone())),
            _ => None,
        })
        .collect();
    kind.finished = kind
        .adapter
        .finish_kind(kind.state.as_ref(), done, complete);
    // After `finish_kind`, which drops what a kind lists once (a commit two clones hold), so
    // only ids a source really repeats are numbered; reported once, on the complete pass.
    for (id, entries) in &mut kind.finished {
        let renamed = make_ids_unique(entries);
        if renamed > 0 && complete {
            diagnostics::warn(
                id.to_string(),
                format!("Numbered {renamed} entries whose id an earlier entry has"),
            );
        }
    }
}

/// The report of where every connection stands: a collected one with its kind's `finished`
/// entries.
fn report(ids: &[ConnectionId], slots: &[Slot], kinds: &BTreeMap<Source, Kind>) -> RunReport {
    let slots = ids
        .iter()
        .zip(slots)
        .map(|(id, slot)| {
            let slot = match slot {
                Slot::Pending(carried) => ReportSlot::Pending {
                    carried: carried.clone(),
                },
                Slot::Skipped => ReportSlot::Skipped,
                Slot::Failed(key, failure) => ReportSlot::Settled(Outcome::Failed {
                    key: *key,
                    failure: failure.clone(),
                }),
                Slot::Kept(outcome) => ReportSlot::Kept(outcome.clone()),
                Slot::Done(key, _) => {
                    let entries = kinds
                        .get(&id.source)
                        .and_then(|kind| kind.finished.iter().find(|(other, _)| other == id))
                        .map(|(_, entries)| entries.as_slice())
                        .unwrap_or_default();
                    ReportSlot::Settled(Outcome::Collected(Stored {
                        key: *key,
                        entries: entries.into(),
                    }))
                }
            };
            (id.clone(), slot)
        })
        .collect();
    RunReport { slots }
}
