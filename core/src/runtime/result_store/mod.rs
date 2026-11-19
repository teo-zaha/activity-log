//! What the shown collection holds per connection, as its last report gave it. The log clients
//! see is folded from it, so carrying, keeping and evicting a connection's entries never has to
//! take a folded log apart again (B19), and every entry carries the key it was collected under
//! (B4).

use std::collections::BTreeSet;

use crate::{
    aggregator::within,
    model::{ActivityLog, CollectOptions, ConnectionId, Source},
    runtime::{
        keys::KeyTable,
        report::{Outcome, RunReport, Slot, Stored},
    },
};

#[cfg(test)]
mod tests;

/// The kinds a run collects; the connections of other kinds keep their outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Scope {
    All,
    /// A kind is collected whole, since its connections can depend on each other (Git matches
    /// emails found in all of them).
    Kinds(BTreeSet<Source>),
}

impl Scope {
    fn collects(&self, source: &Source) -> bool {
        match self {
            Self::All => true,
            Self::Kinds(kinds) => kinds.contains(source),
        }
    }
}

/// What a new run does with one connection, before its check is attached.
#[derive(Debug, Clone)]
pub(crate) enum Seed {
    Collect { carried: Option<Stored> },
    Keep(Outcome),
}

/// One connection of the runtime a run starts on.
pub(crate) struct Candidate {
    pub(crate) id: ConnectionId,
    /// False once its check found it unusable: it is not listed.
    pub(crate) listable: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct ResultStore {
    range: CollectOptions,
    report: RunReport,
}

impl ResultStore {
    pub(crate) fn new(range: CollectOptions, report: RunReport) -> Self {
        Self { range, report }
    }

    pub(crate) fn range(&self) -> CollectOptions {
        self.range
    }

    /// The log clients see, see [`RunReport::project`].
    pub(crate) fn project(&self) -> ActivityLog {
        self.report.project(self.range).log
    }

    /// The outcome the store has for `id`, settled in its run or kept from an earlier one.
    pub(crate) fn outcome(&self, id: &ConnectionId) -> Option<&Outcome> {
        match self.slot(id)? {
            Slot::Settled(outcome) | Slot::Kept(outcome) => Some(outcome),
            Slot::Pending { .. } | Slot::Skipped => None,
        }
    }

    fn slot(&self, id: &ConnectionId) -> Option<&Slot> {
        self.report
            .slots
            .iter()
            .find(|(other, _)| other == id)
            .map(|(_, slot)| slot)
    }

    /// The plan of a new run of `options` over `candidates` (the new runtime's connections, in
    /// order). Connections of kinds in `scope`, and pending ones, are collected, carrying what
    /// this store shows for them; the others keep their outcome, and are left out when the store
    /// has none (skipped or never collected). Unusable connections are left out. Nothing whose
    /// key differs from `keys` is carried or kept.
    pub(crate) fn plan(
        &self,
        options: CollectOptions,
        candidates: &[Candidate],
        scope: &Scope,
        keys: &KeyTable,
    ) -> Vec<(ConnectionId, Seed)> {
        candidates
            .iter()
            .filter_map(|candidate| {
                let id = &candidate.id;
                let key = keys.key(id)?;
                let slot = self.slot(id);
                let pending = matches!(slot, Some(Slot::Pending { .. }));
                if scope.collects(&id.source) || pending {
                    if !candidate.listable {
                        return None;
                    }
                    let shown = match slot {
                        Some(Slot::Pending { carried }) => carried.as_ref(),
                        Some(Slot::Settled(Outcome::Collected(stored)))
                        | Some(Slot::Kept(Outcome::Collected(stored))) => Some(stored),
                        _ => None,
                    };
                    let carried = shown
                        .filter(|stored| stored.key == key)
                        .and_then(|stored| self.carry(stored, options));
                    return Some((id.clone(), Seed::Collect { carried }));
                }
                let outcome = self.outcome(id).filter(|outcome| outcome.key() == key)?;
                Some((id.clone(), Seed::Keep(outcome.clone())))
            })
            .collect()
    }

    /// What a run of `options` carries from `stored`. Collecting the same range again keeps
    /// everything: meetings are collected by overlap, so one that began before the range is
    /// still in it. For another range, an entry stays if it falls inside, a meeting if it
    /// overlaps.
    fn carry(&self, stored: &Stored, options: CollectOptions) -> Option<Stored> {
        if self.range == options {
            return Some(stored.clone());
        }
        let entries = stored
            .entries
            .iter()
            .filter(|entry| within(entry, options))
            .cloned()
            .collect::<Vec<_>>();
        (!entries.is_empty()).then(|| Stored {
            key: stored.key,
            entries: entries.into(),
        })
    }

    /// Drops everything whose key differs from `keys`; true if anything was dropped.
    pub(crate) fn evict(&mut self, keys: &KeyTable) -> bool {
        evict(&mut self.report, keys)
    }
}

/// Drops from `report` every carried, settled or kept outcome whose key differs from `keys`,
/// and every connection `keys` does not have; true if anything was dropped.
pub(crate) fn evict(report: &mut RunReport, keys: &KeyTable) -> bool {
    let before = report.slots.len();
    let mut cleared = false;
    report.slots.retain_mut(|(id, slot)| {
        let Some(key) = keys.key(id) else {
            return false;
        };
        match slot {
            Slot::Pending { carried } => {
                if carried.as_ref().is_some_and(|stored| stored.key != key) {
                    *carried = None;
                    cleared = true;
                }
                true
            }
            Slot::Skipped => true,
            Slot::Settled(outcome) | Slot::Kept(outcome) => outcome.key() == key,
        }
    });
    cleared || report.slots.len() != before
}
