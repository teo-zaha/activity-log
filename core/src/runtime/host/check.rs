//! Connection checks: a recheck, starting a check and recording how it ended.

use std::sync::Arc;

use futures::FutureExt;

use super::{
    Answered, Host, HostInner,
    run::collect_again,
    state::{CheckState, ConnectionCheck, HostState, Shown},
};
use crate::{
    model::{ConnectionId, LogLevel, ProvidersResponse},
    provider::{ProviderRuntime, connection::Connection},
    runtime::{
        keys::KeyChange,
        report::{Checked, Superseded},
    },
};

/// Why [`Host::recheck`] started no check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecheckRefused {
    /// The host has no connection it can check by that id.
    UnknownConnection,
    /// The host has shut down: no check starts any more.
    ShutDown,
}

impl Host {
    /// Runs the check of `id` again and then collects its kind again for the range on screen;
    /// the other kinds keep what they show. A running collection starts over, so it waits for
    /// the new check; a failed one runs whole. Answers with the providers, `id` checking.
    pub fn recheck(
        &self,
        id: &ConnectionId,
    ) -> Result<Answered<ProvidersResponse>, RecheckRefused> {
        let applied = self.apply(|inner, state| {
            if state.shut_down {
                return Err(RecheckRefused::ShutDown);
            }
            let index = state
                .checks
                .iter()
                .position(|check| check.id() == id && check.recheckable)
                .ok_or(RecheckRefused::UnknownConnection)?;
            let connection = state.checks[index].connection.clone();
            let check = new_check(inner, state, &connection);
            let old = std::mem::replace(&mut state.checks[index], check);
            state.retired.push(Box::new(old));
            state.failures.remove(id);
            state.log(LogLevel::Info, id.to_string(), "Checking again");
            collect_again(inner, state, &id.source);
            Ok(state.providers_response())
        });
        let stamp = applied.published.providers_stamp();
        let value = self.spawn(applied)?;
        Ok(Answered { value, stamp })
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
            let (key, change) = state
                .keys
                .record(id, runtime.identity())
                .ok_or(Superseded)?;
            let checked = Checked { runtime, key };
            // A check that asked the store shows how it answered.
            state.secret_store = inner.secrets.store_info();
            let old = std::mem::replace(
                &mut state.checks[index].state,
                CheckState::Recorded(Box::new(checked.clone())),
            );
            state.retired.push(Box::new(old));
            // Another account than the one entries may be shown for: the checked provider and
            // the eviction go out as one pair.
            if change == KeyChange::Replaced {
                state.new_authority = true;
            }
            if change != KeyChange::Kept {
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
}

/// Starts the check of `connection`; it is recorded when it ends (`Host::record_check`) and
/// only then yields to the collections that wait for it. Its work joins the transition's. Its
/// callers return first once the host shut down.
pub(super) fn new_check(
    inner: &Arc<HostInner>,
    state: &mut HostState,
    connection: &Connection,
) -> ConnectionCheck {
    let (checking, check) = (inner.starter)(connection);
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
    ConnectionCheck {
        connection: connection.clone(),
        recheckable: true,
        checking,
        generation,
        state: CheckState::Running(recorded),
    }
}
