use std::{
    any::Any,
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
    aggregator::collect_activity_from_results,
    collect_progressively, diagnostics,
    model::{
        ActivityLog, CollectOptions, CollectRequest, ConnectionId, ProviderInfo, ProviderStatus,
        ProvidersResponse,
    },
    provider::{ProviderRuntime, connection::Connection, current_os, start_checks},
};

#[cfg(test)]
mod tests;

pub type CollectorFuture =
    Pin<Box<dyn Future<Output = AnyhowResult<ActivityLog>> + Send + 'static>>;
/// Receives the partial log each time a connection settles while others are pending.
pub type Progress = Arc<dyn Fn(ActivityLog) + Send + Sync>;
pub type Collector = Arc<dyn Fn(CollectOptions, Progress) -> CollectorFuture + Send + Sync>;
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
}

/// One connection's startup check, shared by everything that waits for it.
type Check = Shared<BoxFuture<'static, ProviderRuntime>>;

enum Providers {
    Checked(Vec<ProviderInfo>),
    /// Each connection with the info that lists it as checking until its check is done.
    Checking(Vec<(ProviderInfo, Check)>),
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
        Self::with_providers(collector, Providers::Checked(provider_info))
    }

    /// The runtime for `connections`, ready before their startup checks finish: until its check
    /// ends a connection is listed as checking, and a collection reads each connection as soon
    /// as its own check is done. The returned task runs the checks and has to be spawned.
    pub fn start(connections: &[Connection]) -> (Self, RuntimeTask) {
        let checks = start_checks(connections)
            .into_iter()
            .map(|(checking, check)| (checking, check.shared()))
            .collect::<Vec<_>>();
        let all_checks = join_all(checks.iter().map(|(_, check)| check.clone())).map(|_| ());
        let collector: Collector = {
            let checks = checks
                .iter()
                .map(|(checking, check)| (checking.connection.clone(), check.clone()))
                .collect::<Vec<_>>();
            Arc::new(move |options, progress| {
                let checks = checks.clone();
                Box::pin(async move {
                    let collected =
                        collect_progressively(options, checks, |log| progress(log)).await;
                    Ok(collected.log)
                })
            })
        };
        let runtime = Self::with_providers(collector, Providers::Checking(checks));
        (runtime, Box::pin(all_checks))
    }

    fn with_providers(collector: Collector, providers: Providers) -> Self {
        Self {
            inner: Arc::new(CollectionRuntimeInner {
                last_run: Mutex::new(LastRun::NotStarted),
                next_run: AtomicU64::new(0),
                providers,
                collector,
            }),
        }
    }

    pub fn provider_info(&self) -> Vec<ProviderInfo> {
        match &self.inner.providers {
            Providers::Checked(info) => info.clone(),
            Providers::Checking(checks) => checks
                .iter()
                .map(|(checking, check)| match check.peek() {
                    Some(runtime) => runtime.info_owned(),
                    None => checking.clone(),
                })
                .collect(),
        }
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

    /// Marks a collection of `options` as running and returns its first snapshot, without
    /// entries, with the work to spawn. While one is already running, nothing starts: its
    /// current snapshot comes back without a task.
    #[must_use]
    pub fn start_collection(
        &self,
        options: CollectOptions,
    ) -> (ActivitySnapshot, Option<RuntimeTask>) {
        let first =
            Arc::new(collect_activity_from_results(options, Vec::new(), self.collectable()).log);
        let run = self.inner.next_run.fetch_add(1, Ordering::Relaxed);
        let (abort, registration) = AbortHandle::new_pair();
        {
            let mut last_run = self.last_run();
            if let LastRun::Running { log, .. } = &*last_run {
                return (ActivitySnapshot::Running(log.clone()), None);
            }
            *last_run = LastRun::Running {
                run,
                options,
                log: first.clone(),
                abort,
            };
        }
        let task = self.clone().run_collection(run, options, registration);
        (ActivitySnapshot::Running(first), Some(Box::pin(task)))
    }

    /// Starts collecting the default range now, so a client asking right away already sees it
    /// running, and returns the work to spawn.
    pub fn initial_collection_task(&self) -> Option<RuntimeTask> {
        self.start_collection(default_collect_options()).1
    }

    /// Continues on this runtime what `previous`, the runtime it replaces, shows: a running
    /// collection is stopped there and started again here, and one that ended, failed or not,
    /// is collected again, so the new connections never answer "No data yet". Returns the work
    /// to spawn.
    ///
    /// Stopping drops the run at its next await; blocking work it already handed to a thread
    /// (the Git walk, WSL) runs to its end unseen.
    #[must_use]
    pub fn take_over(&self, previous: &CollectionRuntime) -> Option<RuntimeTask> {
        let options = match &*previous.last_run() {
            LastRun::Running { options, abort, .. } => {
                abort.abort();
                *options
            }
            LastRun::Done { options, .. } | LastRun::Failed { options, .. } => *options,
            LastRun::NotStarted => return None,
        };
        self.start_collection(options).1
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
        self.inner
            .last_run
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    async fn run_collection(
        self,
        run: u64,
        options: CollectOptions,
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
            AssertUnwindSafe(async move { collector(options, progress).await }).catch_unwind();
        let Ok(outcome) = Abortable::new(work, registration).await else {
            // Stopped by `take_over`: the runtime is being replaced and keeps its last snapshot.
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
    if options.end <= options.start {
        return Err("end must be after start");
    }
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
