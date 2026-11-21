//! What the host holds under its state lock: the installed connections with their checks, the
//! collection on screen, and what a transition starts, retires and logs.

use std::{
    any::Any,
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use futures::{
    FutureExt,
    future::{AbortHandle, BoxFuture, Shared},
};

use super::{HostWork, RunSpawner, config::ReadFailure};
use crate::{
    model::{
        ActivityState, CollectOptions, ConnectionId, LogLevel, ProviderInfo, ProvidersResponse,
        RunId,
    },
    provider::{connection::Connection, current_os, provider_check_result::CheckOutcome},
    runtime::{
        keys::KeyTable,
        report::{Checked, CollectFailure, Superseded},
        result_store::ResultStore,
    },
    settings::ContentId,
};

pub(super) struct HostState {
    /// The installed config's connections with their current checks, in connection order.
    pub(super) checks: Vec<ConnectionCheck>,
    /// The installed config file's content; the last one that did not parse.
    pub(super) content: Option<ContentId>,
    pub(super) rejected: Option<ContentId>,
    /// What the last read of the file failed with, logged once until it fails otherwise.
    pub(super) reload_error: Option<ReadFailure>,
    pub(super) shown: Shown,
    pub(super) activity: Arc<ActivityState>,
    /// The connections whose last collection failed, with the error; shown as their status.
    pub(super) failures: BTreeMap<ConnectionId, CollectFailure>,
    pub(super) keys: KeyTable,
    pub(super) next_run: u64,
    pub(super) next_check: u64,
    /// `None` after shutdown.
    pub(super) spawner: Option<RunSpawner>,
    /// What this transition starts, handed to the spawner once the lock is released.
    pub(super) work: Vec<HostWork>,
    /// What this transition replaced, dropped once the lock is released.
    pub(super) retired: Vec<Box<dyn Any + Send>>,
    /// What this transition logs, once the lock is released: writing to stderr can block.
    pub(super) logs: Vec<(LogLevel, String, String)>,
    /// Set by `shutdown`: no run, check, install or edit is current any more.
    pub(super) shut_down: bool,
    /// This transition is an authority boundary: it installed a config, or a check changed a
    /// connection's collection key. The published authority rises and both parts are stamped,
    /// so a subscriber gets what changed as one pair.
    pub(super) new_authority: bool,
}

type SharedCheck = Shared<BoxFuture<'static, Result<Checked, Superseded>>>;

/// One connection of the installed config and its current check.
pub(super) struct ConnectionCheck {
    pub(super) connection: Connection,
    /// False for a host built from a fixed provider list.
    pub(super) recheckable: bool,
    /// How it is listed until its check is recorded.
    pub(super) checking: ProviderInfo,
    /// Host-wide, so a check of a replaced config or an earlier recheck is never current.
    pub(super) generation: u64,
    pub(super) state: CheckState,
}

pub(super) enum CheckState {
    Running(SharedCheck),
    Recorded(Box<Checked>),
}

impl ConnectionCheck {
    pub(super) fn id(&self) -> &ConnectionId {
        &self.connection.id
    }

    fn info(&self) -> ProviderInfo {
        match &self.state {
            CheckState::Running(_) => self.checking.clone(),
            CheckState::Recorded(checked) => checked.runtime.info_owned(),
        }
    }

    /// Whether a collection starting now waits for it: still checking, or usable.
    pub(super) fn listable(&self) -> bool {
        match &self.state {
            CheckState::Running(_) => true,
            CheckState::Recorded(checked) => checked.runtime.usable(),
        }
    }

    /// The recorded check, once it is recorded.
    pub(super) fn checked(&self) -> BoxFuture<'static, Result<Checked, Superseded>> {
        match &self.state {
            CheckState::Running(check) => check.clone().boxed(),
            CheckState::Recorded(checked) => std::future::ready(Ok((**checked).clone())).boxed(),
        }
    }
}

/// The collection on screen.
pub(super) enum Shown {
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
    pub(super) fn run(&self) -> Option<RunId> {
        match self {
            Self::Empty => None,
            Self::Running { run, .. } | Self::Ready { run, .. } | Self::Failed { run, .. } => {
                Some(*run)
            }
        }
    }

    pub(super) fn store(&self) -> Option<&ResultStore> {
        match self {
            Self::Running { store, .. } | Self::Ready { store, .. } => Some(store),
            Self::Empty | Self::Failed { .. } => None,
        }
    }
}

impl HostState {
    pub(super) fn is_running(&self, run: RunId) -> bool {
        !self.shut_down
            && matches!(&self.shown, Shown::Running { run: running, .. } if *running == run)
    }

    pub(super) fn log(
        &mut self,
        level: LogLevel,
        scope: impl Into<String>,
        message: impl Into<String>,
    ) {
        self.logs.push((level, scope.into(), message.into()));
    }

    pub(super) fn show(&mut self, shown: Shown, activity: ActivityState) {
        let old = std::mem::replace(&mut self.shown, shown);
        self.retired.push(Box::new(old));
        self.set_activity(activity);
    }

    pub(super) fn set_activity(&mut self, activity: ActivityState) {
        let old = std::mem::replace(&mut self.activity, Arc::new(activity));
        self.retired.push(Box::new(old));
    }

    /// Each connection as its check was recorded; one whose last collection failed is
    /// degraded, with the error as its reason.
    pub(super) fn providers_response(&self) -> ProvidersResponse {
        let mut providers = self
            .checks
            .iter()
            .map(ConnectionCheck::info)
            .collect::<Vec<_>>();
        for info in providers.iter_mut().filter(|info| info.usable) {
            if let Some(failure) = self.failures.get(&info.connection) {
                CheckOutcome::Degraded {
                    problem: failure.problem(),
                    reason: format!("Collection failed: {}", failure.message()),
                }
                .apply_to(info);
            }
        }
        ProvidersResponse {
            current_os: current_os(),
            providers,
        }
    }

    /// Drops from the shown store what no longer matches its connection's key and shows the
    /// rest.
    pub(super) fn evict_shown(&mut self) {
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
