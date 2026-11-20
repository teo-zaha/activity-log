//! The host: what outlives one config. It installs configs (an edit, a reload of the file),
//! runs their connections' checks, starts and ends collections and keeps their per-connection
//! results, and publishes what it shows. Every transition is decided, installed and published
//! under one short state lock, so a stopped run, a replaced check or a replaced config can no
//! longer change what shows (B14, B15). Work a transition starts reaches the adapter's spawner
//! only after the lock is released, so a command's answer always shows the state it installed,
//! however fast the work is (B66).

use std::{
    any::Any,
    collections::{BTreeMap, BTreeSet},
    future::Future,
    panic::AssertUnwindSafe,
    path::{Path, PathBuf},
    sync::{Arc, MutexGuard, Weak},
    time::{Duration, Instant},
};

use anyhow::{Context, anyhow};
use futures::{
    FutureExt,
    future::{AbortHandle, AbortRegistration, Abortable, BoxFuture, Shared},
};
use tokio::sync::{oneshot, watch};

use super::{
    default_collect_options,
    keys::KeyTable,
    lock, panic_message,
    report::{Checked, Collector, Outcome, Plan, Progress, RunReport, Slot, Superseded},
    result_store::{Candidate, ResultStore, Scope, Seed, evict},
};
use crate::{
    collect_planned, diagnostics,
    model::{
        ActivityState, CollectOptions, CollectionFailedResponse, ConnectionId, InstallReceipt,
        LogLevel, PartStamp, ProviderInfo, ProviderProblem, ProviderStatus, ProvidersResponse,
        RunEnd, RunId, RunOutcome, RuntimeState, Source,
    },
    provider::{ProviderRuntime, connection::Connection, current_os, start_check},
    settings::{
        ConfigFile, ContentId, EditError, Precondition, edit_config, read_config_bytes,
        warn_no_location,
    },
};

mod published;
#[cfg(test)]
mod tests;

pub use published::{Part, Published};

/// Work the host hands to its adapter: a collection run resolves to its `RunEnd`; a check
/// resolves to `None`. A run is handed over exactly once; a join of a running collection hands
/// over nothing.
pub type HostWork = BoxFuture<'static, Option<RunEnd>>;

/// Spawns host work on the adapter's runtime. Must not block, and must not hold the host or
/// anything that owns it.
pub type RunSpawner = Arc<dyn Fn(HostWork) + Send + Sync>;

/// Starts a connection's check: how it is listed meanwhile, and the check. `None` for a kind
/// no provider handles.
pub(crate) type CheckStarter = Arc<
    dyn Fn(&Connection) -> Option<(ProviderInfo, BoxFuture<'static, ProviderRuntime>)>
        + Send
        + Sync,
>;

/// What [`Host::start`] does with a config file it cannot read or parse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadConfig {
    /// Fail with the error, which names the place and never the file's text: the API server.
    Fail,
    /// Log it and start with no sources, still watching the file: the desktop app.
    StartEmpty,
}

/// What a settings edit installed.
pub struct Edit<T> {
    pub value: T,
    /// The connections of the file as written.
    pub connections: Vec<Connection>,
    /// The providers as installed: a connection whose check runs again is checking.
    pub providers: ProvidersResponse,
    /// The providers part's revision that first holds this edit, or the current one when the
    /// edit changed nothing.
    pub installed: InstallReceipt,
}

/// A command's answer with the stamp of the part it shows, both taken under the state lock.
pub struct Answered<T> {
    pub value: T,
    pub stamp: PartStamp,
}

/// How often the config file's content is compared with the installed one.
const RELOAD_INTERVAL: Duration = Duration::from_secs(2);
/// How long a change another writer made must stay the same before it is installed.
const SETTLE: Duration = Duration::from_millis(200);

#[derive(Clone)]
pub struct Host {
    inner: Arc<HostInner>,
}

struct HostInner {
    /// The config file; `None` without a settings location.
    path: Option<PathBuf>,
    starter: CheckStarter,
    collector: Collector,
    state: std::sync::Mutex<HostState>,
    /// Held from reading or writing the config file until its runtime is installed, so configs
    /// are installed in the order they were written. Taken before the state lock, never after.
    config: Arc<tokio::sync::Mutex<()>>,
    /// Written only under the state lock.
    publisher: watch::Sender<Arc<Published>>,
    shutdown: watch::Sender<bool>,
    #[cfg(test)]
    pauses: std::sync::Mutex<Pauses>,
}

/// Where a test can hold a host's work: each pause signals `reached` and waits for `resume`.
#[cfg(test)]
#[derive(Default)]
pub(crate) struct Pauses {
    /// A reload, after its first read of the file.
    pub(crate) after_first_read: Option<Pause>,
    /// An edit, after its write and before its install.
    pub(crate) after_edit_write: Option<Pause>,
}

#[cfg(test)]
#[derive(Clone)]
pub(crate) struct Pause {
    pub(crate) reached: Arc<tokio::sync::Semaphore>,
    pub(crate) resume: Arc<tokio::sync::Semaphore>,
}

#[cfg(test)]
impl Pause {
    pub(crate) fn new() -> Self {
        Self {
            reached: Arc::new(tokio::sync::Semaphore::new(0)),
            resume: Arc::new(tokio::sync::Semaphore::new(0)),
        }
    }

    async fn hold(&self) {
        self.reached.add_permits(1);
        self.resume.acquire().await.unwrap().forget();
    }
}

struct HostState {
    /// The installed config's connections with their current checks, in connection order.
    checks: Vec<ConnectionCheck>,
    /// The installed config file's content; the last one that did not parse.
    content: Option<ContentId>,
    rejected: Option<ContentId>,
    /// The last error reading the file, logged once.
    reload_error: Option<String>,
    shown: Shown,
    activity: Arc<ActivityState>,
    /// The connections whose last collection failed, with the error; shown as their status.
    failures: BTreeMap<ConnectionId, String>,
    keys: KeyTable,
    next_run: u64,
    next_check: u64,
    /// `None` after shutdown.
    spawner: Option<RunSpawner>,
    /// What this transition starts, handed to the spawner once the lock is released.
    work: Vec<HostWork>,
    /// What this transition replaced, dropped once the lock is released.
    retired: Vec<Box<dyn Any + Send>>,
    /// What this transition logs, once the lock is released: writing to stderr can block.
    logs: Vec<(LogLevel, String, String)>,
    /// Set by `shutdown`: no run, check, install or edit is current any more.
    shut_down: bool,
    /// This transition installed a config: the published authority rises.
    installed: bool,
}

type SharedCheck = Shared<BoxFuture<'static, Result<Checked, Superseded>>>;

/// One connection of the installed config and its current check.
struct ConnectionCheck {
    connection: Connection,
    /// False for a host built from a fixed provider list.
    recheckable: bool,
    /// How it is listed until its check is recorded.
    checking: ProviderInfo,
    /// Host-wide, so a check of a replaced config or an earlier recheck is never current.
    generation: u64,
    state: CheckState,
}

enum CheckState {
    Running(SharedCheck),
    Recorded(Box<Checked>),
}

impl ConnectionCheck {
    fn id(&self) -> &ConnectionId {
        &self.connection.id
    }

    fn info(&self) -> ProviderInfo {
        match &self.state {
            CheckState::Running(_) => self.checking.clone(),
            CheckState::Recorded(checked) => checked.runtime.info_owned(),
        }
    }

    /// Whether a collection starting now waits for it: still checking, or usable.
    fn listable(&self) -> bool {
        match &self.state {
            CheckState::Running(_) => true,
            CheckState::Recorded(checked) => checked.runtime.usable(),
        }
    }

    /// The recorded check, once it is recorded.
    fn checked(&self) -> BoxFuture<'static, Result<Checked, Superseded>> {
        match &self.state {
            CheckState::Running(check) => check.clone().boxed(),
            CheckState::Recorded(checked) => std::future::ready(Ok((**checked).clone())).boxed(),
        }
    }
}

/// The collection on screen.
enum Shown {
    Empty,
    Running {
        run: RunId,
        options: CollectOptions,
        abort: AbortHandle,
        /// The connections it collects; the others it keeps.
        collects: BTreeSet<ConnectionId>,
        store: ResultStore,
    },
    Ready {
        run: RunId,
        store: ResultStore,
    },
    Failed {
        run: RunId,
        options: CollectOptions,
    },
}

impl Shown {
    fn run(&self) -> Option<RunId> {
        match self {
            Self::Empty => None,
            Self::Running { run, .. } | Self::Ready { run, .. } | Self::Failed { run, .. } => {
                Some(*run)
            }
        }
    }

    fn store(&self) -> Option<&ResultStore> {
        match self {
            Self::Running { store, .. } | Self::Ready { store, .. } => Some(store),
            Self::Empty | Self::Failed { .. } => None,
        }
    }
}

impl HostState {
    fn is_running(&self, run: RunId) -> bool {
        !self.shut_down
            && matches!(&self.shown, Shown::Running { run: running, .. } if *running == run)
    }

    fn log(&mut self, level: LogLevel, scope: impl Into<String>, message: impl Into<String>) {
        self.logs.push((level, scope.into(), message.into()));
    }

    fn show(&mut self, shown: Shown, activity: ActivityState) {
        let old = std::mem::replace(&mut self.shown, shown);
        self.retired.push(Box::new(old));
        self.set_activity(activity);
    }

    fn set_activity(&mut self, activity: ActivityState) {
        let old = std::mem::replace(&mut self.activity, Arc::new(activity));
        self.retired.push(Box::new(old));
    }

    /// Each connection as its check was recorded; one whose last collection failed is
    /// degraded, with the error as its reason.
    fn providers_response(&self) -> ProvidersResponse {
        let mut providers = self
            .checks
            .iter()
            .map(ConnectionCheck::info)
            .collect::<Vec<_>>();
        for info in providers.iter_mut().filter(|info| info.usable) {
            if let Some(message) = self.failures.get(&info.connection) {
                info.status = ProviderStatus::Degraded;
                info.problem = Some(ProviderProblem::Failed);
                info.reason = Some(format!("Collection failed: {message}"));
            }
        }
        ProvidersResponse {
            current_os: current_os(),
            providers,
        }
    }

    /// Drops from the shown store what no longer matches its connection's key and shows the
    /// rest.
    fn evict_shown(&mut self) {
        let (Shown::Running { store, .. } | Shown::Ready { store, .. }) = &mut self.shown else {
            return;
        };
        if !store.evict(&self.keys) {
            return;
        }
        let log = Arc::new(store.project());
        let activity = match &self.shown {
            Shown::Running { .. } => ActivityState::Running { log },
            _ => ActivityState::Ready { log },
        };
        self.set_activity(activity);
    }
}

#[cfg(test)]
thread_local! {
    static STATE_HELD: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether this thread holds a host's state lock; tests check that nothing logs under it.
#[cfg(test)]
pub(crate) fn state_lock_held() -> bool {
    STATE_HELD.get()
}

/// The state lock, held.
struct StateGuard<'a>(MutexGuard<'a, HostState>);

impl Drop for StateGuard<'_> {
    fn drop(&mut self) {
        #[cfg(test)]
        STATE_HELD.set(false);
    }
}

impl std::ops::Deref for StateGuard<'_> {
    type Target = HostState;

    fn deref(&self) -> &HostState {
        &self.0
    }
}

impl std::ops::DerefMut for StateGuard<'_> {
    fn deref_mut(&mut self) -> &mut HostState {
        &mut self.0
    }
}

/// What a transition installed: its answer, the value it published and the work it starts.
struct Applied<R> {
    answer: R,
    published: Arc<Published>,
    work: Vec<HostWork>,
    spawner: Option<RunSpawner>,
}

impl Host {
    /// The host of the config file at `path`: its connections are checked at once and the file
    /// is watched once [`Self::watch_config`] runs. `path` is `None` when there is no settings
    /// location: no sources, nothing watched, and every edit fails with
    /// [`EditError::NoLocation`].
    pub fn start(
        path: Option<PathBuf>,
        spawner: RunSpawner,
        bad_config: BadConfig,
    ) -> anyhow::Result<Host> {
        let host = Self::build(
            path.clone(),
            Arc::new(start_check),
            planned_collector(),
            spawner,
        );
        let Some(path) = path else {
            warn_no_location();
            host.install(Vec::new(), None);
            return Ok(host);
        };
        match load(&path) {
            Ok((connections, content)) => host.install(connections, Some(content)),
            Err((err, content)) => match bad_config {
                BadConfig::Fail => {
                    host.shutdown();
                    return Err(err);
                }
                BadConfig::StartEmpty => {
                    diagnostics::error("settings", format!("Reading settings failed: {err:#}"));
                    host.lock().rejected = content;
                    host.install(Vec::new(), None);
                }
            },
        }
        Ok(host)
    }

    /// A host of fixed connections that never reloads, for tests.
    pub fn fixed(connections: &[Connection], spawner: RunSpawner) -> Host {
        let host = Self::build(None, Arc::new(start_check), planned_collector(), spawner);
        host.install(connections.to_vec(), None);
        host
    }

    /// A host of providers whose checks are done, collected by `collector`, for tests.
    pub fn with_collector(
        collector: Collector,
        providers: Vec<ProviderInfo>,
        spawner: RunSpawner,
    ) -> Host {
        let host = Self::build(None, Arc::new(|_| None), collector, spawner);
        host.transition(|_, state| {
            let connections = providers
                .iter()
                .map(|info| {
                    Connection::new(
                        info.connection.clone(),
                        info.label.clone(),
                        Default::default(),
                        Default::default(),
                    )
                })
                .collect::<Vec<_>>();
            state.keys.install(&connections);
            state.checks = connections
                .into_iter()
                .zip(providers)
                .map(|(connection, info)| {
                    let key = state.keys.key(&connection.id).expect("just installed");
                    ConnectionCheck {
                        connection,
                        recheckable: false,
                        checking: info.clone(),
                        generation: 0,
                        state: CheckState::Recorded(Box::new(Checked {
                            runtime: ProviderRuntime::from_info(info),
                            key,
                        })),
                    }
                })
                .collect();
        });
        host
    }

    pub(crate) fn build(
        path: Option<PathBuf>,
        starter: CheckStarter,
        collector: Collector,
        spawner: RunSpawner,
    ) -> Host {
        let empty = ProvidersResponse {
            current_os: current_os(),
            providers: Vec::new(),
        };
        let initial = Arc::new(Published::initial(empty));
        let state = HostState {
            checks: Vec::new(),
            content: None,
            rejected: None,
            reload_error: None,
            shown: Shown::Empty,
            activity: initial.activity.value.clone(),
            failures: BTreeMap::new(),
            keys: KeyTable::default(),
            next_run: 0,
            next_check: 0,
            spawner: Some(spawner),
            work: Vec::new(),
            retired: Vec::new(),
            logs: Vec::new(),
            shut_down: false,
            installed: false,
        };
        Host {
            inner: Arc::new(HostInner {
                path,
                starter,
                collector,
                state: std::sync::Mutex::new(state),
                config: Arc::default(),
                publisher: watch::Sender::new(initial),
                shutdown: watch::Sender::new(false),
                #[cfg(test)]
                pauses: Default::default(),
            }),
        }
    }

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

    /// Runs the check of `id` again and then collects its kind again for the range on screen;
    /// the other kinds keep what they show. A running collection starts over, so it waits for
    /// the new check; a failed one runs whole. Answers with the providers, `id` checking; `None`
    /// for a connection the host does not have.
    pub fn recheck(&self, id: &ConnectionId) -> Option<Answered<ProvidersResponse>> {
        let applied = self.apply(|inner, state| {
            let index = state
                .checks
                .iter()
                .position(|check| check.id() == id && check.recheckable)?;
            let connection = state.checks[index].connection.clone();
            let check = new_check(inner, state, &connection)?;
            let old = std::mem::replace(&mut state.checks[index], check);
            state.retired.push(Box::new(old));
            state.failures.remove(id);
            state.log(LogLevel::Info, id.to_string(), "Checking again");
            collect_again(inner, state, &id.source);
            Some(state.providers_response())
        });
        let stamp = applied.published.providers_stamp();
        let value = self.spawn(applied)?;
        Some(Answered { value, stamp })
    }

    /// Edits the config file (see [`edit_config`]) and installs the result. The write and the
    /// install run in a task of their own, so dropping this future never leaves a written file
    /// uninstalled.
    pub async fn edit<T: Send + 'static>(
        &self,
        precondition: Precondition,
        change: impl FnOnce(&mut ConfigFile) -> anyhow::Result<T> + Send + 'static,
    ) -> Result<Edit<T>, EditError> {
        let Some(path) = self.inner.path.clone() else {
            return Err(EditError::NoLocation);
        };
        let host = self.clone();
        let (answer, answered) = oneshot::channel();
        tokio::spawn(async move {
            let _ = answer.send(host.edit_owned(path, precondition, change).await);
        });
        answered
            .await
            .unwrap_or_else(|_| Err(EditError::Io(anyhow!("Saving the settings stopped"))))
    }

    async fn edit_owned<T: Send + 'static>(
        self,
        path: PathBuf,
        precondition: Precondition,
        change: impl FnOnce(&mut ConfigFile) -> anyhow::Result<T> + Send + 'static,
    ) -> Result<Edit<T>, EditError> {
        let config = self.inner.config.clone().lock_owned().await;
        if self.lock().shut_down {
            return Err(EditError::Io(anyhow!(
                "Activity Log is quitting; the settings were not saved"
            )));
        }
        // The config lock may wait for another process, so it is not taken on an async worker.
        let edited = tokio::task::spawn_blocking(move || edit_config(&path, precondition, change))
            .await
            .map_err(|err| EditError::Io(anyhow!("Saving the settings failed: {err}")))??;
        #[cfg(test)]
        self.pause(|pauses| pauses.after_edit_write.clone()).await;
        for warning in &edited.warnings {
            diagnostics::warn("config", warning);
        }
        let connections = edited
            .file
            .connections()
            .map_err(EditError::Rejected)?
            .connections;
        let installed = self.lock().content == Some(edited.content);
        let applied = if installed {
            // Nothing changed anywhere: the providers as they are.
            self.apply(|_, state| state.providers_response())
        } else {
            self.apply_install(connections.clone(), Some(edited.content))
        };
        drop(config);
        let installed = applied.published.receipt();
        let providers = self.spawn(applied);
        Ok(Edit {
            value: edited.value,
            connections,
            providers,
            installed,
        })
    }

    /// Installs the config file if its content is not the installed one. A change another
    /// writer made is installed once it read the same twice, [`SETTLE`] apart; content that does
    /// not parse is logged once and the installed config stays.
    pub async fn reload_if_changed(&self) {
        let Some(path) = self.inner.path.clone() else {
            return;
        };
        let config = self.inner.config.clone().lock_owned().await;
        if self.lock().shut_down {
            return;
        }
        let Some(bytes) = self.read_config(&path).await else {
            return;
        };
        let content = ContentId::of(&bytes);
        {
            let state = self.lock();
            if state.content == Some(content) || state.rejected == Some(content) {
                return;
            }
        }
        #[cfg(test)]
        self.pause(|pauses| pauses.after_first_read.clone()).await;
        tokio::time::sleep(SETTLE).await;
        if self.read_config(&path).await.as_ref() != Some(&bytes) {
            // Still being written: the next look installs it.
            return;
        }
        let parsed = ConfigFile::parse(&path, &bytes)
            .map_err(anyhow::Error::from)
            .and_then(|file| file.logged_connections());
        let connections = match parsed {
            Ok(connections) => connections,
            Err(err) => {
                diagnostics::error(
                    "config",
                    format!("Could not reload {}: {err:#}", path.display()),
                );
                self.lock().rejected = Some(content);
                return;
            }
        };
        diagnostics::info("config", format!("Reloaded {}", path.display()));
        let applied = self.apply_install(connections, Some(content));
        drop(config);
        self.spawn(applied);
    }

    /// Calls [`Self::reload_if_changed`] every 2 s until shutdown or until the host is gone;
    /// holds the host weakly. Ends at once without a config file.
    pub fn watch_config(&self) -> impl Future<Output = ()> + Send + 'static {
        let host = Arc::downgrade(&self.inner);
        let watched = self.inner.path.is_some();
        let mut stop = self.inner.shutdown.subscribe();
        async move {
            if !watched || *stop.borrow_and_update() {
                return;
            }
            loop {
                tokio::select! {
                    () = tokio::time::sleep(RELOAD_INTERVAL) => {}
                    _ = stop.changed() => return,
                }
                let Some(inner) = host.upgrade() else {
                    return;
                };
                Host { inner }.reload_if_changed().await;
            }
        }
    }

    /// Stops the watcher, stops the shown run and drops the spawner, so nothing the host
    /// starts runs any more.
    /// The host's last transition: the shown run stops, every run and check still in flight
    /// is no longer current (it shows nothing and a run ends `Stopped`), no edit or reload
    /// installs anything, and the spawner is dropped. What shows stays as it was.
    pub fn shutdown(&self) {
        let retired = {
            let mut state = self.lock();
            state.shut_down = true;
            if let Shown::Running { abort, .. } = &state.shown {
                abort.abort();
            }
            let spawner = state.spawner.take();
            (spawner, std::mem::take(&mut state.work))
        };
        self.inner.shutdown.send_replace(true);
        drop(retired);
    }

    /// The collection on screen.
    pub fn activity(&self) -> ActivityState {
        (*self.inner.publisher.borrow().activity.value).clone()
    }

    pub fn providers_response(&self) -> ProvidersResponse {
        (*self.inner.publisher.borrow().providers.value).clone()
    }

    /// Both parts as one coherent pair, with the host's stamps.
    pub fn runtime_state(&self) -> RuntimeState {
        self.inner.publisher.borrow().runtime_state()
    }

    /// Whether `run` is still the collection on screen.
    pub fn is_current(&self, run: RunId) -> bool {
        let state = self.lock();
        !state.shut_down && state.shown.run() == Some(run)
    }

    /// The published state: the current value, then each one a transition installs.
    pub fn subscribe(&self) -> watch::Receiver<Arc<Published>> {
        self.inner.publisher.subscribe()
    }

    #[cfg(test)]
    async fn pause(&self, which: impl FnOnce(&Pauses) -> Option<Pause>) {
        let pause = which(&lock(&self.inner.pauses));
        if let Some(pause) = pause {
            pause.hold().await;
        }
    }

    #[cfg(test)]
    pub(crate) fn set_pauses(&self, pauses: Pauses) {
        *lock(&self.inner.pauses) = pauses;
    }

    fn lock(&self) -> StateGuard<'_> {
        let guard = lock(&self.inner.state);
        #[cfg(test)]
        STATE_HELD.set(true);
        StateGuard(guard)
    }

    /// Applies `change` and publishes what it changed, under the state lock; what it retired is
    /// dropped after the lock is released. The work it starts is returned, not spawned.
    fn apply<R>(&self, change: impl FnOnce(&Arc<HostInner>, &mut HostState) -> R) -> Applied<R> {
        let (applied, retired) = {
            let mut guard = self.lock();
            let state = &mut *guard;
            let answer = change(&self.inner, state);
            let current = self.inner.publisher.borrow().clone();
            if let Some(next) = current.next(
                current.revision(),
                &state.activity,
                state.providers_response(),
                std::mem::take(&mut state.installed),
            ) {
                let old = self.inner.publisher.send_replace(Arc::new(next));
                state.retired.push(Box::new(old));
            }
            let applied = Applied {
                answer,
                published: self.inner.publisher.borrow().clone(),
                work: std::mem::take(&mut state.work),
                spawner: state.spawner.clone(),
            };
            let after = (
                std::mem::take(&mut state.retired),
                std::mem::take(&mut state.logs),
            );
            (applied, after)
        };
        let (retired, logs) = retired;
        drop(retired);
        for (level, scope, message) in logs {
            diagnostics::record(level, scope, message);
        }
        applied
    }

    /// Hands the work of `applied` to the spawner and returns its answer. Called with no lock
    /// held.
    fn spawn<R>(&self, applied: Applied<R>) -> R {
        if let Some(spawner) = applied.spawner {
            for work in applied.work {
                spawner(work);
            }
        }
        applied.answer
    }

    fn transition<R>(&self, change: impl FnOnce(&Arc<HostInner>, &mut HostState) -> R) -> R {
        let applied = self.apply(change);
        self.spawn(applied)
    }

    /// Installs a config's connections. Their checks start again; what the shown collection
    /// does follows the kinds the change affects, see [`affected_kinds`].
    fn install(&self, connections: Vec<Connection>, content: Option<ContentId>) {
        let applied = self.apply_install(connections, content);
        self.spawn(applied);
    }

    fn apply_install(
        &self,
        connections: Vec<Connection>,
        content: Option<ContentId>,
    ) -> Applied<ProvidersResponse> {
        self.apply(|inner, state| {
            if state.shut_down {
                return state.providers_response();
            }
            if content.is_some() {
                state.content = content;
            }
            state.installed = true;
            let old_ids = state
                .checks
                .iter()
                .map(|check| check.id().clone())
                .collect::<Vec<_>>();
            let changed = state.keys.install(&connections);
            let checks = connections
                .iter()
                .filter_map(|connection| new_check(inner, state, connection))
                .collect::<Vec<_>>();
            let old = std::mem::replace(&mut state.checks, checks);
            state.retired.push(Box::new(old));
            let new_ids = state
                .checks
                .iter()
                .map(|check| check.id().clone())
                .collect::<Vec<_>>();
            let affected = affected_kinds(&old_ids, &new_ids, &changed, state.shown.store());
            state
                .failures
                .retain(|id, _| new_ids.contains(id) && !affected.contains(&id.source));
            state.evict_shown();
            let run = match &state.shown {
                Shown::Running { options, .. } | Shown::Failed { options, .. } => {
                    Some((*options, Scope::All))
                }
                Shown::Ready { store, .. } if !affected.is_empty() => {
                    Some((store.range(), Scope::Kinds(affected)))
                }
                Shown::Ready { .. } | Shown::Empty => None,
            };
            if let Some((options, scope)) = run {
                start_run(inner, state, options, scope);
            }
            state.providers_response()
        })
    }

    /// Records a check of `id` that `generation` started, if it is still the connection's
    /// current one. A changed key drops what was collected under the old one and collects the
    /// connection's kind again if the shown collection does not.
    fn record_check(
        &self,
        id: &ConnectionId,
        generation: u64,
        runtime: ProviderRuntime,
    ) -> Result<Checked, Superseded> {
        self.transition(|inner, state| {
            if state.shut_down {
                return Err(Superseded);
            }
            let index = state
                .checks
                .iter()
                .position(|check| check.generation == generation)
                .ok_or(Superseded)?;
            let (key, changed) = state
                .keys
                .record(id, runtime.identity())
                .ok_or(Superseded)?;
            let checked = Checked { runtime, key };
            let old = std::mem::replace(
                &mut state.checks[index].state,
                CheckState::Recorded(Box::new(checked.clone())),
            );
            state.retired.push(Box::new(old));
            if changed {
                // Another account: what failed for the previous one is not its failure.
                state.failures.remove(id);
                state.evict_shown();
                let collected = match &state.shown {
                    Shown::Running { collects, .. } => collects.contains(id),
                    Shown::Ready { .. } => false,
                    Shown::Empty | Shown::Failed { .. } => true,
                };
                if !collected {
                    collect_again(inner, state, &id.source);
                }
            }
            Ok(checked)
        })
    }

    fn progress(&self, run: RunId, options: CollectOptions, mut report: RunReport) {
        let log = report.project(options).log;
        self.transition(|_, state| {
            if !state.is_running(run) {
                return;
            }
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
    fn finish(
        &self,
        run: RunId,
        options: CollectOptions,
        result: std::thread::Result<anyhow::Result<RunReport>>,
    ) -> RunOutcome {
        let message = match result {
            Ok(Ok(report)) => return self.finish_ready(run, options, report),
            Ok(Err(err)) => format!("{err:#}"),
            Err(panic) => format!(
                "Collection stopped unexpectedly: {}",
                panic_message(&*panic)
            ),
        };
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
            let log = if evict(&mut report, &state.keys) {
                report.project(options).log
            } else {
                log
            };
            for (id, slot) in &report.slots {
                match slot {
                    Slot::Settled(Outcome::Failed { message, .. }) => {
                        state.failures.insert(id.clone(), message.clone());
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

    /// Reads the config file's bytes, a missing file as empty ones. A failure is logged once
    /// per distinct message and tried again on the next look.
    async fn read_config(&self, path: &Path) -> Option<Vec<u8>> {
        let owned = path.to_path_buf();
        let read = tokio::task::spawn_blocking(move || read_config_bytes(&owned)).await;
        let message = match read {
            Ok(Ok(bytes)) => {
                self.lock().reload_error = None;
                return Some(bytes.unwrap_or_default());
            }
            Ok(Err(err)) => format!("Could not read {}: {err}", path.display()),
            Err(err) => format!("Could not read {}: {err}", path.display()),
        };
        let first = {
            let mut state = self.lock();
            let first = state.reload_error.as_ref() != Some(&message);
            state.reload_error = Some(message.clone());
            first
        };
        if first {
            diagnostics::error("config", message);
        }
        None
    }
}

/// Reads the connections of the config file at `path`, with the content they were read from
/// when there was any.
fn load(path: &Path) -> Result<(Vec<Connection>, ContentId), (anyhow::Error, Option<ContentId>)> {
    let bytes = read_config_bytes(path)
        .with_context(|| format!("Reading {}", path.display()))
        .map_err(|err| (err, None))?
        .unwrap_or_default();
    let content = ContentId::of(&bytes);
    let connections = ConfigFile::parse(path, &bytes)
        .map_err(anyhow::Error::from)
        .and_then(|file| file.logged_connections())
        .map_err(|err| (err, Some(content)))?;
    Ok((connections, content))
}

fn planned_collector() -> Collector {
    Arc::new(|options, plans, progress: Progress| {
        async move { Ok(collect_planned(options, plans, |report| progress(report)).await) }.boxed()
    })
}

/// Starts the check of `connection`; it is recorded when it ends (`Host::record_check`) and
/// only then yields to the collections that wait for it. Its work joins the transition's.
fn new_check(
    inner: &Arc<HostInner>,
    state: &mut HostState,
    connection: &Connection,
) -> Option<ConnectionCheck> {
    if state.shut_down {
        return None;
    }
    let (checking, check) = (inner.starter)(connection)?;
    state.next_check += 1;
    let generation = state.next_check;
    let id = connection.id.clone();
    let host = Arc::downgrade(inner);
    let recorded = async move {
        let runtime = check.await;
        match host.upgrade() {
            Some(inner) => Host { inner }.record_check(&id, generation, runtime),
            None => Err(Superseded),
        }
    }
    .boxed()
    .shared();
    state.work.push(recorded.clone().map(|_| None).boxed());
    Some(ConnectionCheck {
        connection: connection.clone(),
        recheckable: true,
        checking,
        generation,
        state: CheckState::Running(recorded),
    })
}

/// Collects `source` again for the range on screen: the kind alone after a finished collection,
/// everything after a failed one or over a running one, nothing before the first one.
fn collect_again(inner: &Arc<HostInner>, state: &mut HostState, source: &Source) {
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
fn start_run(inner: &Arc<HostInner>, state: &mut HostState, options: CollectOptions, scope: Scope) {
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

/// The kinds a config change affects: one of its connections was added, removed or reordered,
/// changed its values or location, or has no collected or failed outcome in `store` (it was
/// skipped or never collected, and its new check may make it usable). A label or access-only
/// change affects nothing.
fn affected_kinds(
    old: &[ConnectionId],
    new: &[ConnectionId],
    changed: &BTreeSet<ConnectionId>,
    store: Option<&ResultStore>,
) -> BTreeSet<Source> {
    let of_kind = |ids: &[ConnectionId], kind: &Source| {
        ids.iter()
            .filter(|id| id.source == *kind)
            .cloned()
            .collect::<Vec<_>>()
    };
    old.iter()
        .chain(new)
        .map(|id| id.source.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter(|kind| {
            of_kind(old, kind) != of_kind(new, kind)
                || of_kind(new, kind).iter().any(|id| {
                    changed.contains(id) || store.and_then(|store| store.outcome(id)).is_none()
                })
        })
        .collect()
}
