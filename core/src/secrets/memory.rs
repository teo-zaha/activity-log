//! A store in memory for tests: it holds a key or none, answers each call as scripted, and
//! counts what it was asked.

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

use anyhow::anyhow;
use futures::{FutureExt, future::BoxFuture};
use tokio::sync::Semaphore;

use super::{
    Created, DataKey, Interaction, KeyCreator, Lookup, SecretStore, StoreError, StoreErrorKind,
    StoreKind,
};

/// A store call, for scripting its answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreCall {
    Read,
    Create,
    Probe,
}

/// How often each call was made.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct StoreCalls {
    pub reads: usize,
    pub creates: usize,
    pub probes: usize,
}

pub struct MemoryStore {
    kind: StoreKind,
    /// Whether this program may create the key here (see [`Self::read_only`]).
    creates: bool,
    state: Mutex<MemoryState>,
    /// While set, every read waits for a permit (see [`Self::hold_reads`]).
    read_gate: Mutex<Option<Arc<Semaphore>>>,
}

#[derive(Default)]
struct MemoryState {
    key: Option<DataKey>,
    failures: VecDeque<(StoreCall, StoreErrorKind)>,
    /// What the next creation stores in place of the key it is given: another writer's.
    created_instead: Option<DataKey>,
    /// The next read panics, as a broken store's task would.
    panic_next_read: bool,
    calls: StoreCalls,
    read_interactions: Vec<Interaction>,
}

impl MemoryStore {
    pub fn new(kind: StoreKind) -> Self {
        Self {
            kind,
            creates: true,
            state: Mutex::default(),
            read_gate: Mutex::new(None),
        }
    }

    pub fn holding(kind: StoreKind, key: DataKey) -> Self {
        let store = Self::new(kind);
        store.state().key = Some(key);
        store
    }

    /// A store this program only reads: it offers no creation.
    pub fn read_only(mut self) -> Self {
        self.creates = false;
        self
    }

    /// The next `call` fails with `kind`; failures queue up in order.
    pub fn fail_next(&self, call: StoreCall, kind: StoreErrorKind) {
        self.state().failures.push_back((call, kind));
    }

    /// The next creation stores `key`, not the one it is given, as if another writer had.
    pub fn create_instead(&self, key: DataKey) {
        self.state().created_instead = Some(key);
    }

    /// The next read panics.
    pub fn panic_next_read(&self) {
        self.state().panic_next_read = true;
    }

    /// Replaces the key held, as another process or the user would.
    pub fn set_key(&self, key: Option<DataKey>) {
        self.state().key = key;
    }

    pub fn key(&self) -> Option<DataKey> {
        self.state().key.clone()
    }

    pub fn calls(&self) -> StoreCalls {
        self.state().calls
    }

    pub fn read_interactions(&self) -> Vec<Interaction> {
        self.state().read_interactions.clone()
    }

    /// Makes every read wait until [`ReadGate::release`]: concurrent readers can be seen
    /// sharing one.
    pub fn hold_reads(&self) -> ReadGate {
        let gate = Arc::new(Semaphore::new(0));
        *lock(&self.read_gate) = Some(gate.clone());
        ReadGate(gate)
    }

    fn state(&self) -> MutexGuard<'_, MemoryState> {
        lock(&self.state)
    }

    fn scripted_failure(&self, state: &mut MemoryState, call: StoreCall) -> Option<StoreError> {
        let index = state.failures.iter().position(|(next, _)| *next == call)?;
        let (_, kind) = state.failures.remove(index)?;
        Some(StoreError::new(
            kind,
            anyhow!("Scripted {kind:?} for {call:?}"),
        ))
    }
}

/// Holds reads until released.
pub struct ReadGate(Arc<Semaphore>);

impl ReadGate {
    pub fn release(&self) {
        self.0.add_permits(Semaphore::MAX_PERMITS / 2);
    }
}

impl SecretStore for MemoryStore {
    fn kind(&self) -> StoreKind {
        self.kind
    }

    fn read(&self, interaction: Interaction) -> BoxFuture<'_, Result<Lookup, StoreError>> {
        async move {
            {
                let mut state = self.state();
                state.calls.reads += 1;
                state.read_interactions.push(interaction);
            }
            let gate = lock(&self.read_gate).clone();
            if let Some(gate) = gate {
                gate.acquire().await.expect("the gate stays open").forget();
            }
            let mut state = self.state();
            if std::mem::take(&mut state.panic_next_read) {
                drop(state);
                panic!("a scripted panic while reading the key");
            }
            if let Some(err) = self.scripted_failure(&mut state, StoreCall::Read) {
                return Err(err);
            }
            Ok(match &state.key {
                Some(key) => Lookup::Found(key.clone()),
                None => Lookup::Absent,
            })
        }
        .boxed()
    }

    fn probe(&self) -> BoxFuture<'_, Result<(), StoreError>> {
        async move {
            let mut state = self.state();
            state.calls.probes += 1;
            match self.scripted_failure(&mut state, StoreCall::Probe) {
                Some(err) => Err(err),
                None => Ok(()),
            }
        }
        .boxed()
    }

    fn creator(&self) -> Option<&dyn KeyCreator> {
        self.creates.then_some(self as &dyn KeyCreator)
    }
}

impl KeyCreator for MemoryStore {
    fn create_if_absent<'a>(
        &'a self,
        key: &'a DataKey,
    ) -> BoxFuture<'a, Result<Created, StoreError>> {
        async move {
            let mut state = self.state();
            state.calls.creates += 1;
            if let Some(err) = self.scripted_failure(&mut state, StoreCall::Create) {
                return Err(err);
            }
            if state.key.is_some() {
                return Ok(Created::AlreadyThere);
            }
            let stored = state.created_instead.take().unwrap_or_else(|| key.clone());
            state.key = Some(stored);
            Ok(Created::Stored)
        }
        .boxed()
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
