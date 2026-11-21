//! Installing a config's connections, and the kinds a change affects.

use std::collections::BTreeSet;

use super::{Host, check::new_check, run::start_run, state::Shown, transition::Applied};
use crate::{
    model::{ConnectionId, ProvidersResponse, Source},
    provider::connection::Connection,
    runtime::result_store::{ResultStore, Scope},
    settings::ContentId,
};

#[cfg(test)]
mod tests;

/// Whether what a host shows holds a config file once an install was asked for.
pub(super) enum InstallOutcome {
    /// It was installed, or it already was: the providers as they show it.
    Shown(ProvidersResponse),
    /// The host shut down first and installs nothing: the providers it shows without it.
    ShutDown(ProvidersResponse),
}

impl Host {
    /// Installs a config's connections. Their checks start again; what the shown collection
    /// does follows the kinds the change affects, see [`affected_kinds`].
    pub(super) fn install(&self, connections: Vec<Connection>, content: Option<ContentId>) {
        let applied = self.apply_install(connections, content);
        self.spawn(applied);
    }

    /// Installs `connections`, unless the host shut down.
    pub(super) fn apply_install(
        &self,
        connections: Vec<Connection>,
        content: Option<ContentId>,
    ) -> Applied<InstallOutcome> {
        self.apply(|inner, state| {
            if state.shut_down {
                return InstallOutcome::ShutDown(state.providers_response());
            }
            if content.is_some() {
                state.content = content;
            }
            state.new_authority = true;
            let old_ids = state
                .checks
                .iter()
                .map(|check| check.id().clone())
                .collect::<Vec<_>>();
            let changed = state.keys.install(&connections);
            let checks = connections
                .iter()
                .map(|connection| new_check(inner, state, connection))
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
            InstallOutcome::Shown(state.providers_response())
        })
    }
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
