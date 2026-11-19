use std::{
    any::Any,
    collections::BTreeMap,
    future::Future,
    panic::AssertUnwindSafe,
    pin::Pin,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicU64, Ordering},
    },
};

use anyhow::Result as AnyhowResult;
use chrono::{DateTime, Duration, Utc};
use futures::{
    FutureExt,
    future::{AbortHandle, AbortRegistration, Abortable, BoxFuture, Shared, join_all},
};

use crate::{
    Plan,
    aggregator::{carried_for, carry_over, collect_activity_from_results, entries_by_connection},
    collect_planned, diagnostics,
    model::{
        ActivityLog, CollectOptions, CollectRequest, ConnectionId, ProviderInfo, ProviderProblem,
        ProviderStatus, ProvidersResponse, Source,
    },
    provider::{
        ProviderRuntime, connection::Connection, current_os, provider_adapter::ConnectionEntries,
        start_check,
    },
};

#[cfg(test)]
mod tests;

pub type CollectorFuture =
    Pin<Box<dyn Future<Output = AnyhowResult<ActivityLog>> + Send + 'static>>;
/// Receives the partial log each time a connection settles while others are pending.
pub type Progress = Arc<dyn Fn(ActivityLog) + Send + Sync>;
/// What the log on screen had per connection when a collection started, see `carry_over`.
pub type Carried = Arc<Vec<ConnectionEntries>>;
pub type Collector =
    Arc<dyn Fn(CollectOptions, Carried, Scope, Progress) -> CollectorFuture + Send + Sync>;

/// Which connections a collection reads.
#[derive(Clone)]
pub enum Scope {
    All,
    /// Only the connections of one kind; the others keep what the log on screen had for them
    /// (`kept`). A kind is collected whole, since its connections can depend on each other (Git
    /// matches emails found in all of them).
    Kind {
        source: Source,
        kept: Arc<Vec<ConnectionEntries>>,
    },
}

impl Scope {
    fn collects(&self, id: &ConnectionId) -> bool {
        match self {
            Self::All => true,
            Self::Kind { source, .. } => id.source == *source,
        }
    }
}
pub type RuntimeTask = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

#[derive(Clone)]
pub struct CollectionRuntime {
    inner: Arc<CollectionRuntimeInner>,
}

struct CollectionRuntimeInner {
    last_run: Mutex<LastRun>,
    next_run: AtomicU64,
    providers: Providers,
    collector: Collector,
    /// The connections whose last collection failed, with the error; shown as their status.
    failures: Failures,
}

type Failures = Arc<Mutex<BTreeMap<ConnectionId, String>>>;

/// One connection's startup check, shared by everything that waits for it.
type Check = Shared<BoxFuture<'static, ProviderRuntime>>;

/// A connection, the info that lists it as checking until its check is done, and that check;
/// `recheck` replaces the check.
struct ConnectionCheck {
    connection: Connection,
    checking: ProviderInfo,
    check: Check,
}

type Checks = Arc<Mutex<Vec<ConnectionCheck>>>;

enum Providers {
    Checked(Vec<ProviderInfo>),
    Checking(Checks),
}

/// What the most recent collection is doing, or how it ended.
enum LastRun {
    NotStarted,
    Running {
        /// Tells this run from a later one, so a late progress report or result is dropped.
        run: u64,
        options: CollectOptions,
        /// What is collected so far; `meta.pending` names the rest.
        log: Arc<ActivityLog>,
        abort: AbortHandle,
    },
    Done {
        options: CollectOptions,
        log: Arc<ActivityLog>,
    },
    Failed {
        options: CollectOptions,
        message: String,
    },
}

impl LastRun {
    /// The log a client sees now: the partial one of a running collection or the last complete
    /// one.
    fn shown(&self) -> Option<&Arc<ActivityLog>> {
        match self {
            Self::Running { log, .. } | Self::Done { log, .. } => Some(log),
            Self::NotStarted | Self::Failed { .. } => None,
        }
    }
}

pub enum ActivitySnapshot {
    /// A partial log: `meta.pending` names the connections still to come.
    Running(Arc<ActivityLog>),
    Ready(Arc<ActivityLog>),
    Failed {
        options: CollectOptions,
        message: String,
    },
    Empty,
}

impl CollectionRuntime {
    pub fn new(collector: Collector, provider_info: Vec<ProviderInfo>) -> Self {
        Self::with_providers(
            collector,
            Providers::Checked(provider_info),
            Failures::default(),
        )
    }

    /// The runtime for `connections`, ready before their startup checks finish: until its check
    /// ends a connection is listed as checking, and a collection reads each connection as soon
    /// as its own check is done. The returned task runs the checks and has to be spawned.
    pub fn start(connections: &[Connection]) -> (Self, RuntimeTask) {
        let checks = connections
            .iter()
            .filter_map(|connection| {
                let (checking, check) = start_check(connection)?;
                Some(ConnectionCheck {
                    connection: connection.clone(),
                    checking,
                    check: check.shared(),
                })
            })
            .collect::<Vec<_>>();
        let all_checks = join_all(checks.iter().map(|check| check.check.clone())).map(|_| ());
        let checks: Checks = Arc::new(Mutex::new(checks));
        let failures = Failures::default();
        let collector = check_collector(checks.clone(), failures.clone());
        let runtime = Self::with_providers(collector, Providers::Checking(checks), failures);
        (runtime, Box::pin(all_checks))
    }

    fn with_providers(collector: Collector, providers: Providers, failures: Failures) -> Self {
        Self {
            inner: Arc::new(CollectionRuntimeInner {
                last_run: Mutex::new(LastRun::NotStarted),
                next_run: AtomicU64::new(0),
                providers,
                collector,
                failures,
            }),
        }
    }

    /// Each connection as its startup check found it; one whose last collection failed is
    /// degraded, with the error as its reason.
    pub fn provider_info(&self) -> Vec<ProviderInfo> {
        let mut info = match &self.inner.providers {
            Providers::Checked(info) => info.clone(),
            Providers::Checking(checks) => lock(checks)
                .iter()
                .map(|check| match check.check.peek() {
                    Some(runtime) => runtime.info_owned(),
                    None => check.checking.clone(),
                })
                .collect(),
        };
        let failures = lock(&self.inner.failures);
        for info in info.iter_mut().filter(|info| info.usable) {
            if let Some(message) = failures.get(&info.connection) {
                info.status = ProviderStatus::Degraded;
                info.problem = Some(ProviderProblem::Failed);
                info.reason = Some(format!("Collection failed: {message}"));
            }
        }
        info
    }

    /// Runs the startup check of `id` again and, once it is done, collects its kind again for
    /// the range on screen; the other connections keep what they show. A collection that is
    /// running starts over, so it waits for the new check; one that failed is run again whole.
    /// Returns the work to spawn, or `None` for a connection this runtime does not know.
    #[must_use]
    pub fn recheck(&self, id: &ConnectionId) -> Option<RuntimeTask> {
        let Providers::Checking(checks) = &self.inner.providers else {
            return None;
        };
        let check = {
            let mut checks = lock(checks);
            let entry = checks
                .iter_mut()
                .find(|check| check.checking.connection == *id)?;
            let (checking, check) = start_check(&entry.connection)?;
            entry.checking = checking;
            entry.check = check.shared();
            entry.check.clone()
        };
        lock(&self.inner.failures).remove(id);
        diagnostics::info(id.to_string(), "Checking again");

        let collection = {
            let last_run = self.last_run();
            match &*last_run {
                LastRun::NotStarted => None,
                LastRun::Running { options, log, .. } => {
                    Some((*options, Some(log.clone()), Scope::All))
                }
                LastRun::Failed { options, .. } => Some((*options, None, Scope::All)),
                LastRun::Done { options, log } => Some((
                    *options,
                    Some(log.clone()),
                    Scope::Kind {
                        source: id.source.clone(),
                        kept: Arc::new(kept_except(log, &id.source)),
                    },
                )),
            }
        };
        let collection = collection.and_then(|(options, shown, scope)| {
            self.start_run(options, Some(shown), scope, true).1
        });
        Some(Box::pin(async move {
            // The check runs whether or not a collection waits for it.
            let check = check.map(|_| ());
            match collection {
                Some(collection) => {
                    futures::join!(check, collection);
                }
                None => check.await,
            }
        }))
    }

    pub fn providers_response(&self) -> ProvidersResponse {
        ProvidersResponse {
            current_os: current_os(),
            providers: self.provider_info(),
        }
    }

    pub fn is_collecting(&self) -> bool {
        matches!(*self.last_run(), LastRun::Running { .. })
    }

    pub fn activity_snapshot(&self) -> ActivitySnapshot {
        match &*self.last_run() {
            LastRun::NotStarted => ActivitySnapshot::Empty,
            LastRun::Running { log, .. } => ActivitySnapshot::Running(log.clone()),
            LastRun::Done { log, .. } => ActivitySnapshot::Ready(log.clone()),
            LastRun::Failed { options, message } => ActivitySnapshot::Failed {
                options: *options,
                message: message.clone(),
            },
        }
    }

    /// Marks a collection of `options` as running and returns its first snapshot with the work to
    /// spawn. A collection of the same range that is already running goes on: its current
    /// snapshot comes back without a task. One of another range is stopped, like in
    /// `take_over`, so the last range asked for is the one collected. Until a connection is in,
    /// the snapshots show what the log on screen had for it (see `carry_over`).
    #[must_use]
    pub fn start_collection(
        &self,
        options: CollectOptions,
    ) -> (ActivitySnapshot, Option<RuntimeTask>) {
        self.start_run(options, None, Scope::All, false)
    }

    /// Starts collecting the default range now, so a client asking right away already sees it
    /// running, and returns the work to spawn.
    pub fn initial_collection_task(&self) -> Option<RuntimeTask> {
        self.start_collection(default_collect_options()).1
    }

    /// Continues on this runtime what `previous`, the runtime it replaces, shows: a running
    /// collection is stopped there and started again here, and one that ended, failed or not,
    /// is collected again, so the new connections never answer "No data yet". What `previous`
    /// shows stays on screen until each connection is in again. Returns the work to spawn.
    ///
    /// Stopping drops the run at its next await. Blocking work it already handed to a thread
    /// sees it (`platform::blocking`): the local Git walk stops between folders, repositories
    /// and commits, a WSL script's `wsl.exe` is killed, Outlook and event log reads stop between
    /// items. A call into Outlook that does not return holds Outlook until it does; the next
    /// collection waits for it.
    #[must_use]
    pub fn take_over(&self, previous: &CollectionRuntime) -> Option<RuntimeTask> {
        let (options, shown) = {
            let last_run = previous.last_run();
            let options = match &*last_run {
                LastRun::Running { options, abort, .. } => {
                    abort.abort();
                    *options
                }
                LastRun::Done { options, .. } | LastRun::Failed { options, .. } => *options,
                LastRun::NotStarted => return None,
            };
            (options, last_run.shown().cloned())
        };
        self.start_run(options, Some(shown), Scope::All, false).1
    }

    /// Starts a run of the connections in `scope` that carries from `shown`, or from this
    /// runtime's own log when `None`; that log is read under the lock that replaces it, so no
    /// progress of the old run lands between. A run of the same range goes on unless `restart`.
    fn start_run(
        &self,
        options: CollectOptions,
        shown: Option<Option<Arc<ActivityLog>>>,
        scope: Scope,
        restart: bool,
    ) -> (ActivitySnapshot, Option<RuntimeTask>) {
        let pending = self
            .collectable()
            .into_iter()
            .filter(|id| scope.collects(id))
            .collect::<Vec<_>>();
        let kept = match &scope {
            Scope::All => Vec::new(),
            Scope::Kind { kept, .. } => kept.to_vec(),
        };
        let run = self.inner.next_run.fetch_add(1, Ordering::Relaxed);
        let (abort, registration) = AbortHandle::new_pair();
        let (first, carried) = {
            let mut last_run = self.last_run();
            if let LastRun::Running {
                options: running,
                log,
                abort,
                ..
            } = &*last_run
            {
                if *running == options && !restart {
                    return (ActivitySnapshot::Running(log.clone()), None);
                }
                abort.abort();
                if *running != options {
                    diagnostics::info(
                        "runtime",
                        format!(
                            "Stopped collecting {} to {} for another range",
                            running.start.to_rfc3339(),
                            running.end.to_rfc3339()
                        ),
                    );
                }
            }
            let shown = shown.unwrap_or_else(|| last_run.shown().cloned());
            let carried: Carried =
                Arc::new(shown.map_or_else(Vec::new, |log| carry_over(&log, options)));
            let first = Arc::new(
                collect_activity_from_results(
                    options,
                    kept.clone(),
                    pending.clone(),
                    carried_for(&carried, &pending, &kept),
                )
                .log,
            );
            *last_run = LastRun::Running {
                run,
                options,
                log: first.clone(),
                abort,
            };
            (first, carried)
        };
        let task = self
            .clone()
            .run_collection(run, options, carried, scope, registration);
        (ActivitySnapshot::Running(first), Some(Box::pin(task)))
    }

    /// The connections a collection starting now waits for.
    fn collectable(&self) -> Vec<ConnectionId> {
        self.provider_info()
            .into_iter()
            .filter(|info| info.usable || info.status == ProviderStatus::Checking)
            .map(|info| info.connection)
            .collect()
    }

    fn last_run(&self) -> MutexGuard<'_, LastRun> {
        lock(&self.inner.last_run)
    }

    async fn run_collection(
        self,
        run: u64,
        options: CollectOptions,
        carried: Carried,
        scope: Scope,
        registration: AbortRegistration,
    ) {
        let progress: Progress = {
            let runtime = self.clone();
            Arc::new(move |log| {
                let log = Arc::new(log);
                runtime.update_run(run, |last_run| {
                    if let LastRun::Running { log: shown, .. } = last_run {
                        *shown = log;
                    }
                });
            })
        };
        let collector = self.inner.collector.clone();
        let work =
            AssertUnwindSafe(async move { collector(options, carried, scope, progress).await })
                .catch_unwind();
        let Ok(outcome) = Abortable::new(work, registration).await else {
            // Stopped by `take_over` or by a collection of another range, which shows instead.
            return;
        };
        let message = match outcome {
            Ok(Ok(log)) => {
                let log = Arc::new(log);
                self.update_run(run, |last_run| *last_run = LastRun::Done { options, log });
                return;
            }
            Ok(Err(err)) => format!("{err:#}"),
            Err(panic) => format!(
                "Collection stopped unexpectedly: {}",
                panic_message(&*panic)
            ),
        };
        diagnostics::error("runtime", format!("Collection failed: {message}"));
        self.update_run(run, |last_run| {
            *last_run = LastRun::Failed { options, message }
        });
    }

    /// Applies `update` if `run` is still the collection that is running.
    fn update_run(&self, run: u64, update: impl FnOnce(&mut LastRun)) {
        let mut last_run = self.last_run();
        if matches!(&*last_run, LastRun::Running { run: running, .. } if *running == run) {
            update(&mut last_run);
        }
    }
}

/// The collector of a runtime that checks its connections: collects the connections in the
/// scope as their current checks end, and records which of them failed.
fn check_collector(checks: Checks, failures: Failures) -> Collector {
    Arc::new(move |options, carried: Carried, scope: Scope, progress| {
        let plans = lock(&checks)
            .iter()
            .filter_map(|check| {
                let id = check.checking.connection.clone();
                if scope.collects(&id) {
                    return Some((id, Plan::Collect(check.check.clone())));
                }
                // Out of the scope: listed only if the log on screen lists it.
                let Scope::Kind { kept, .. } = &scope else {
                    return None;
                };
                let (_, entries) = kept.iter().find(|(kept, _)| *kept == id)?;
                Some((id, Plan::Keep(entries.clone())))
            })
            .collect::<Vec<_>>();
        let failures = failures.clone();
        Box::pin(async move {
            let collected: Vec<ConnectionId> = plans
                .iter()
                .filter(|(_, plan)| matches!(plan, Plan::Collect(_)))
                .map(|(id, _)| id.clone())
                .collect();
            let collection = collect_planned(options, plans, &carried, |log| progress(log)).await;
            let mut failures = lock(&failures);
            for id in &collected {
                failures.remove(id);
            }
            failures.extend(collection.failed);
            Ok(collection.activity.log)
        })
    })
}

/// What `log` lists for the connections of other kinds than `source`, each one it counts even
/// without entries.
fn kept_except(log: &ActivityLog, source: &Source) -> Vec<ConnectionEntries> {
    let mut entries = entries_by_connection(log);
    log.summary
        .connections
        .iter()
        .filter(|count| count.connection.source != *source)
        .map(|count| {
            let id = count.connection.clone();
            let found = entries
                .iter_mut()
                .find(|(other, _)| *other == id)
                .map(|(_, entries)| std::mem::take(entries))
                .unwrap_or_default();
            (id, found)
        })
        .collect()
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn panic_message(panic: &(dyn Any + Send)) -> &str {
    panic
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| panic.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("panic")
}

pub fn resolve_collect_options(
    request: &CollectRequest,
) -> std::result::Result<CollectOptions, &'static str> {
    let parse = |value: &str| {
        DateTime::parse_from_rfc3339(value)
            .map(|instant| instant.with_timezone(&Utc))
            .map_err(|_| "start and end must be RFC 3339 timestamps")
    };
    let options = CollectOptions {
        start: parse(&request.start)?,
        end: parse(&request.end)?,
    };
    options.check()?;
    Ok(options)
}

/// The last 30 days up to now; the dashboard re-collects whole local days when asked.
fn default_collect_options() -> CollectOptions {
    let end = Utc::now();
    CollectOptions {
        start: end - Duration::days(30),
        end,
    }
}
