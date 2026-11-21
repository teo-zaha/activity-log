//! The host: what outlives one config. It installs configs (an edit, a reload of the file),
//! runs their connections' checks, starts and ends collections and keeps their per-connection
//! results, and publishes what it shows. Every transition is decided, installed and published
//! under one short state lock, so a stopped run, a replaced check or a replaced config can no
//! longer change what shows (B14, B15). Work a transition starts reaches the adapter's spawner
//! only after the lock is released, so a command's answer always shows the state it installed,
//! however fast the work is (B66).

use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

use futures::future::BoxFuture;
use tokio::sync::watch;

use super::{
    keys::KeyTable,
    report::{Checked, Collector},
};
use crate::{
    diagnostics,
    model::{
        ActivityState, PartStamp, ProviderInfo, ProvidersResponse, RunEnd, RunId, RuntimeState,
    },
    provider::{ProviderRuntime, connection::Connection, current_os, start_check},
    settings::warn_no_location,
};

mod check;
mod config;
mod feed;
mod install;
#[cfg(test)]
mod pauses;
mod published;
mod run;
mod state;
#[cfg(test)]
mod tests;
mod transition;

pub use check::RecheckRefused;
pub use config::Edit;
pub use feed::Feed;
pub use published::{Part, Published};
#[cfg(test)]
pub(crate) use transition::state_lock_held;

use config::load;
use run::planned_collector;
use state::{CheckState, ConnectionCheck, HostState, Shown};

/// Work the host hands to its adapter: a collection run resolves to its `RunEnd`; a check
/// resolves to `None`. A run is handed over exactly once; a join of a running collection hands
/// over nothing.
pub type HostWork = BoxFuture<'static, Option<RunEnd>>;

/// Spawns host work on the adapter's runtime. Must not block, and must not hold the host or
/// anything that owns it.
pub type RunSpawner = Arc<dyn Fn(HostWork) + Send + Sync>;

/// Starts a connection's check: how it is listed meanwhile, and the check.
pub(crate) type CheckStarter =
    Arc<dyn Fn(&Connection) -> (ProviderInfo, BoxFuture<'static, ProviderRuntime>) + Send + Sync>;

/// What [`Host::start`] does with a config file it cannot read or parse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadConfig {
    /// Fail with the error, which names the place and never the file's text: the API server.
    Fail,
    /// Log it and start with no sources, still watching the file: the desktop app.
    StartEmpty,
}

/// A command's answer with the stamp of the part it shows, both taken under the state lock.
pub struct Answered<T> {
    pub value: T,
    pub stamp: PartStamp,
}

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
    pauses: std::sync::Mutex<pauses::Pauses>,
}

impl Host {
    /// The host of the config file at `path`: its connections are checked at once and the file
    /// is watched once [`Self::watch_config`] runs. `path` is `None` when there is no settings
    /// location: no sources, nothing watched, and every edit fails with
    /// [`EditError::NoLocation`](crate::settings::EditError::NoLocation).
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
        // Its checks are recorded and cannot be run again, and it has no file to install from,
        // so it never starts one.
        let host = Self::build(None, Arc::new(start_check), collector, spawner);
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
            new_authority: false,
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
}
