//! How a transition runs: under the state lock, publishing what it changed, its work handed to
//! the spawner and its logs written only once the lock is released.

use std::sync::{Arc, MutexGuard};

use super::{Host, HostInner, HostWork, Published, RunSpawner, state::HostState};
use crate::{diagnostics, runtime::lock};

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
pub(super) struct StateGuard<'a>(MutexGuard<'a, HostState>);

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
pub(super) struct Applied<R> {
    answer: R,
    pub(super) published: Arc<Published>,
    work: Vec<HostWork>,
    spawner: Option<RunSpawner>,
}

impl Host {
    pub(super) fn lock(&self) -> StateGuard<'_> {
        let guard = lock(&self.inner.state);
        #[cfg(test)]
        STATE_HELD.set(true);
        StateGuard(guard)
    }

    /// Applies `change` and publishes what it changed, under the state lock; what it retired is
    /// dropped after the lock is released. The work it starts is returned, not spawned.
    pub(super) fn apply<R>(
        &self,
        change: impl FnOnce(&Arc<HostInner>, &mut HostState) -> R,
    ) -> Applied<R> {
        let (applied, retired) = {
            let mut guard = self.lock();
            let state = &mut *guard;
            let answer = change(&self.inner, state);
            let current = self.inner.publisher.borrow().clone();
            if let Some(next) = current.next(
                current.revision(),
                &state.activity,
                state.providers_response(),
                std::mem::take(&mut state.new_authority),
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
    pub(super) fn spawn<R>(&self, applied: Applied<R>) -> R {
        if let Some(spawner) = applied.spawner {
            for work in applied.work {
                spawner(work);
            }
        }
        applied.answer
    }

    pub(super) fn transition<R>(
        &self,
        change: impl FnOnce(&Arc<HostInner>, &mut HostState) -> R,
    ) -> R {
        let applied = self.apply(change);
        self.spawn(applied)
    }
}
