//! Collection runs: starting one, showing its progress and how it ended.

use std::{
    collections::BTreeSet,
    panic::AssertUnwindSafe,
    sync::{Arc, Weak},
    time::Instant,
};

use futures::{
    FutureExt,
    future::{AbortHandle, AbortRegistration, Abortable},
};

use super::{
    Answered, Host, HostInner, HostWork,
    state::{CheckState, ConnectionCheck, HostState, Shown},
};
use crate::{
    collect_planned,
    model::{
        ActivityState, CollectOptions, CollectionFailedResponse, ConnectionId, IdentityEvidence,
        LogLevel, RunEnd, RunId, RunOutcome, Source,
    },
    provider::provider_check_result::{CheckOutcome, ProviderCheckResult},
    runtime::{
        default_collect_options,
        keys::KeyChange,
        panic_message,
        report::{
            CollectFailure, CollectionKey, Collector, Outcome, Plan, Progress, RunReport, Slot,
        },
        result_store::{Candidate, ResultStore, Scope, Seed, evict},
    },
};

impl Host {
    /// Starts collecting the default range now, so a client asking right away already sees it
    /// running.
    pub fn start_initial_collection(&self) {
        self.collect(default_collect_options());
    }

    /// Starts collecting `options`, or joins the running collection of the same range; answers
    /// with the state this call installed. A running collection of another range is stopped,
    /// so the last range asked for is the one collected. Until a connection is in, the log
    /// shows what the log on screen had for it.
    ///
    /// Stopping drops the run at its next await. Blocking work it already handed to a thread
    /// sees it (`platform::blocking`): the local Git walk stops between folders, repositories
    /// and commits, a WSL script's `wsl.exe` is killed, Outlook and event log reads stop between
    /// items. A call into Outlook that does not return holds Outlook until it does; the next
    /// collection waits for it.
    pub fn collect(&self, options: CollectOptions) -> Answered<ActivityState> {
        let applied = self.apply(|inner, state| {
            let joins = matches!(
                &state.shown,
                Shown::Running { options: running, .. } if *running == options
            );
            if !joins {
                start_run(inner, state, options, Scope::All);
            }
            (*state.activity).clone()
        });
        let stamp = applied.published.activity_stamp();
        Answered {
            value: self.spawn(applied),
            stamp,
        }
    }

    pub(super) fn progress(&self, run: RunId, options: CollectOptions, mut report: RunReport) {
        let log = report.project(options).log;
        self.transition(|_, state| {
            if !state.is_running(run) {
                return;
            }
            record_collection_denials(state, &report);
            let log = if evict(&mut report, &state.keys) {
                report.project(options).log
            } else {
                log
            };
            if let Shown::Running { store, .. } = &mut state.shown {
                let old = std::mem::replace(store, ResultStore::new(options, report));
                state.retired.push(Box::new(old));
            }
            state.set_activity(ActivityState::Running { log: Arc::new(log) });
        });
    }

    /// Shows how `run` ended, if it is still the collection on screen.
    pub(super) fn finish(
        &self,
        run: RunId,
        options: CollectOptions,
        result: std::thread::Result<RunReport>,
    ) -> RunOutcome {
        let panic = match result {
            Ok(report) => return self.finish_ready(run, options, report),
            Err(panic) => panic,
        };
        let message = format!(
            "Collection stopped unexpectedly: {}",
            panic_message(&*panic)
        );
        self.transition(|_, state| {
            if !state.is_running(run) {
                return RunOutcome::Stopped;
            }
            state.log(
                LogLevel::Error,
                "runtime",
                format!("Collection failed: {message}"),
            );
            let failure = CollectionFailedResponse::new(&options, message);
            state.show(
                Shown::Failed { run, options },
                ActivityState::Failed { failure },
            );
            RunOutcome::Failed
        })
    }

    fn finish_ready(
        &self,
        run: RunId,
        options: CollectOptions,
        mut report: RunReport,
    ) -> RunOutcome {
        let log = report.project(options).log;
        self.transition(|_, state| {
            if !state.is_running(run) {
                return RunOutcome::Stopped;
            }
            record_collection_denials(state, &report);
            let log = if evict(&mut report, &state.keys) {
                report.project(options).log
            } else {
                log
            };
            for (id, slot) in &report.slots {
                match slot {
                    Slot::Settled(Outcome::Failed { failure, .. }) => {
                        state.failures.insert(id.clone(), failure.clone());
                    }
                    Slot::Kept(_) => {}
                    _ => {
                        state.failures.remove(id);
                    }
                }
            }
            let entries = log.entries.len();
            state.show(
                Shown::Ready {
                    run,
                    store: ResultStore::new(options, report),
                },
                ActivityState::Ready { log: Arc::new(log) },
            );
            RunOutcome::Ready { entries }
        })
    }
}

/// Consume evidence from a current run before projecting it under the resulting keys.
/// The progress and final reports can contain the same failure. Its old key stops it from
/// being recorded again.
fn record_collection_denials(state: &mut HostState, report: &RunReport) {
    for (id, slot) in &report.slots {
        if let Slot::Settled(Outcome::Failed { key, failure, .. }) = slot {
            record_collection_denial(state, id, *key, failure);
        }
    }
}

fn record_collection_denial(
    state: &mut HostState,
    id: &ConnectionId,
    planned: CollectionKey,
    failure: &CollectFailure,
) {
    let identity = failure.identity();
    if *identity == IdentityEvidence::None || state.keys.key(id) != Some(planned) {
        return;
    }
    let Some(check) = state.checks.iter_mut().find(|check| check.id() == id) else {
        return;
    };
    let CheckState::Recorded(checked) = &mut check.state else {
        return;
    };
    if checked.key != planned || !checked.runtime.usable() {
        return;
    }
    let Some((key, change)) = state.keys.record(id, identity) else {
        return;
    };
    if change == KeyChange::Kept {
        return;
    }
    checked.key = key;
    checked.runtime.record_check_result(ProviderCheckResult {
        outcome: CheckOutcome::Unavailable {
            problem: failure.classification(),
            reason: format!("Collection failed: {}", failure.message()),
        },
        identity: identity.clone(),
    });
    state.failures.remove(id);
    state.new_authority |= change == KeyChange::Replaced;
    state.log(
        LogLevel::Warn,
        id.to_string(),
        "Invalidated the previous account's collection key",
    );
}

pub(super) fn planned_collector() -> Collector {
    Arc::new(|options, plans, progress: Progress| {
        async move { collect_planned(options, plans, |report| progress(report)).await }.boxed()
    })
}

/// Collects `source` again for the range on screen: the kind alone after a finished collection,
/// everything after a failed one or over a running one, nothing before the first one.
pub(super) fn collect_again(inner: &Arc<HostInner>, state: &mut HostState, source: &Source) {
    let run = match &state.shown {
        Shown::Empty => None,
        Shown::Running { options, .. } | Shown::Failed { options, .. } => {
            Some((*options, Scope::All))
        }
        Shown::Ready { store, .. } => Some((
            store.range(),
            Scope::Kinds(BTreeSet::from([source.clone()])),
        )),
    };
    if let Some((options, scope)) = run {
        start_run(inner, state, options, scope);
    }
}

/// Shows a new run of `options` over `scope`, carrying from the store on screen, and adds its
/// work to the transition. A running collection is stopped.
pub(super) fn start_run(
    inner: &Arc<HostInner>,
    state: &mut HostState,
    options: CollectOptions,
    scope: Scope,
) {
    if state.shut_down {
        return;
    }
    if let Shown::Running {
        abort,
        options: running,
        ..
    } = &state.shown
    {
        abort.abort();
        if *running != options {
            let message = format!(
                "Stopped collecting {} to {} for another range",
                running.start.to_rfc3339(),
                running.end.to_rfc3339()
            );
            state.log(LogLevel::Info, "runtime", message);
        }
    }
    let candidates = state
        .checks
        .iter()
        .map(|check| Candidate {
            id: check.id().clone(),
            listable: check.listable(),
        })
        .collect::<Vec<_>>();
    let seeds = match state.shown.store() {
        Some(store) => store.plan(options, &candidates, &scope, &state.keys),
        None => ResultStore::new(options, RunReport::default()).plan(
            options,
            &candidates,
            &Scope::All,
            &state.keys,
        ),
    };
    state.next_run += 1;
    let run = RunId(state.next_run);
    let mut plans = Vec::new();
    let mut slots = Vec::new();
    let mut collects = BTreeSet::new();
    for (id, seed) in seeds {
        match seed {
            Seed::Collect { carried } => {
                let check = state
                    .checks
                    .iter()
                    .find(|check| *check.id() == id)
                    .map(ConnectionCheck::checked)
                    .expect("a planned connection has a check");
                plans.push((
                    id.clone(),
                    Plan::Collect {
                        check,
                        carried: carried.clone(),
                    },
                ));
                slots.push((id.clone(), Slot::Pending { carried }));
                collects.insert(id);
            }
            Seed::Keep(outcome) => {
                plans.push((id.clone(), Plan::Keep(outcome.clone())));
                slots.push((id, Slot::Kept(outcome)));
            }
        }
    }
    let store = ResultStore::new(options, RunReport { slots });
    let log = Arc::new(store.project());
    let (abort, registration) = AbortHandle::new_pair();
    state.show(
        Shown::Running {
            run,
            options,
            abort,
            collects,
            store,
        },
        ActivityState::Running { log },
    );
    state.work.push(run_work(
        Arc::downgrade(inner),
        inner.collector.clone(),
        run,
        options,
        plans,
        registration,
    ));
}

/// A run's work: collects `plans`, shows each report while it is the run on screen, then shows
/// how it ended before it resolves.
fn run_work(
    host: Weak<HostInner>,
    collector: Collector,
    run: RunId,
    options: CollectOptions,
    plans: Vec<(ConnectionId, Plan)>,
    registration: AbortRegistration,
) -> HostWork {
    async move {
        let started = Instant::now();
        let progress: Progress = {
            let host = host.clone();
            Arc::new(move |report| {
                if let Some(inner) = host.upgrade() {
                    Host { inner }.progress(run, options, report);
                }
            })
        };
        let work = AssertUnwindSafe(async move { collector(options, plans, progress).await })
            .catch_unwind();
        let outcome = match Abortable::new(work, registration).await {
            Err(_) => RunOutcome::Stopped,
            Ok(result) => match host.upgrade() {
                Some(inner) => Host { inner }.finish(run, options, result),
                None => RunOutcome::Stopped,
            },
        };
        Some(RunEnd {
            run,
            elapsed: started.elapsed(),
            outcome,
        })
    }
    .boxed()
}
