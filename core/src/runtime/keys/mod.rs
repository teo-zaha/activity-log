//! Whose entries a connection's collection holds. A connection's collection key changes with
//! its values, its location or the principal its check found, so entries collected under one
//! key are never carried, kept or shown under another (B4).

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use crate::{
    model::{ConnectionId, IdentityEvidence, RemoteIdentity},
    provider::connection::{Connection, ConnectionLocation, ConnectionValues},
};

#[cfg(test)]
mod tests;

/// Names the inputs a connection's entries were collected under, within one host. Opaque: a
/// host counter, never derived from a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CollectionKey(u64);

impl CollectionKey {
    /// For collections outside a host, such as the CLI's, which keep nothing between runs.
    pub const UNKEYED: Self = Self(0);
}

/// How a recorded check changed a connection's collection key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KeyChange {
    /// The check said what was known already, or nothing usable.
    Kept,
    /// The first answer for these values. Nothing is held under the key before it: a
    /// connection is collected only once its check is recorded.
    First,
    /// Another answer than the one entries may be held under: whose entries they are changed.
    Replaced,
}

/// What the checks of a connection's current values said about whose token it is.
#[derive(Debug, Clone, PartialEq, Eq)]
enum IdentityState {
    /// No check of these values has said anything yet.
    Unknown,
    Verified(RemoteIdentity),
    /// The remote accepted the token without naming its owner. The number is new for every such
    /// check, so two unnamed checks are never the same state.
    Unnamed(u64),
    /// The last check was refused (401/403).
    Denied,
}

struct Entry {
    values: Arc<ConnectionValues>,
    location: ConnectionLocation,
    instance: Option<String>,
    identity: IdentityState,
    key: CollectionKey,
}

/// The current collection key of each connection of the installed config.
#[derive(Default)]
pub(crate) struct KeyTable {
    entries: BTreeMap<ConnectionId, Entry>,
    next: u64,
}

impl KeyTable {
    pub(crate) fn key(&self, id: &ConnectionId) -> Option<CollectionKey> {
        self.entries.get(id).map(|entry| entry.key)
    }

    /// Takes the connections of a newly installed config. A connection whose values,
    /// location and table `instance` are unchanged keeps its key and identity; any other starts
    /// over, unknown, with a new key. Another instance is another connection (removed and added
    /// again); a legacy table that gets its first instance is the same one. Connections that
    /// are gone are dropped. Returns the connections whose key is new.
    pub(crate) fn install(&mut self, connections: &[Connection]) -> BTreeSet<ConnectionId> {
        let mut previous = std::mem::take(&mut self.entries);
        let mut changed = BTreeSet::new();
        for connection in connections {
            let kept = previous.remove(&connection.id).filter(|entry| {
                entry.location == connection.location
                    && entry
                        .instance
                        .as_deref()
                        .is_none_or(|instance| connection.instance() == Some(instance))
                    && (Arc::ptr_eq(&entry.values, connection.shared_values())
                        || *entry.values == *connection.values())
            });
            let entry = match kept {
                Some(mut entry) => {
                    entry.instance = connection.instance().map(str::to_string);
                    entry
                }
                None => {
                    changed.insert(connection.id.clone());
                    Entry {
                        values: connection.shared_values().clone(),
                        location: connection.location.clone(),
                        instance: connection.instance().map(str::to_string),
                        identity: IdentityState::Unknown,
                        key: self.next_key(),
                    }
                }
            };
            self.entries.insert(connection.id.clone(), entry);
        }
        changed
    }

    /// Records what a check of `id` said about its token's owner and returns the connection's
    /// key with how it changed; `None` for a connection not installed. A check that read
    /// something replaces the state; one without a usable answer keeps it.
    pub(crate) fn record(
        &mut self,
        id: &ConnectionId,
        evidence: &IdentityEvidence,
    ) -> Option<(CollectionKey, KeyChange)> {
        let identity = match evidence {
            IdentityEvidence::Verified(identity) => Some(IdentityState::Verified(identity.clone())),
            IdentityEvidence::Unnamed => {
                self.next += 1;
                Some(IdentityState::Unnamed(self.next))
            }
            IdentityEvidence::Denied => Some(IdentityState::Denied),
            IdentityEvidence::None => None,
        };
        let current = self.entries.get(id)?;
        let Some(identity) = identity.filter(|identity| *identity != current.identity) else {
            return Some((current.key, KeyChange::Kept));
        };
        let change = if current.identity == IdentityState::Unknown {
            KeyChange::First
        } else {
            KeyChange::Replaced
        };
        let key = self.next_key();
        let entry = self.entries.get_mut(id)?;
        entry.identity = identity;
        entry.key = key;
        Some((key, change))
    }

    fn next_key(&mut self) -> CollectionKey {
        self.next += 1;
        CollectionKey(self.next)
    }
}
