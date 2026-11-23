//! Secrets sealed in the file, and what keeps them right: sealing the plain ones, refusing a
//! change of what a sealed one is bound to, and opening what was sealed before it is
//! published.

use toml_edit::{InlineTable, Item, Value};

use super::{ConfigFile, read::entry_values};
use crate::{
    model::ConnectionId,
    provider::{
        adapter as adapter_of,
        connection::{StoredSecret, StoredValue},
        provider_config::ProviderConfigKey,
        provider_registry,
    },
    secrets::{
        Sealer,
        sealed::{Binding, Unopenable},
    },
    settings::{SEALED, field_name},
};

/// What a sealing pass did.
#[derive(Debug, Default)]
pub(in crate::settings) struct Sealed {
    pub(in crate::settings) count: usize,
    /// Plain secrets left as they were, their table naming no address they can be bound to.
    pub(in crate::settings) unbound: Vec<(ConnectionId, ProviderConfigKey)>,
}

impl ConfigFile {
    /// Replaces each secret the file holds as plain text with it sealed by `sealer` for the
    /// binding its table names. It marks no table written, so it gives none an `instance`.
    pub(in crate::settings) fn seal_plain(&mut self, sealer: &Sealer) -> Sealed {
        let mut sealed = Sealed::default();
        for adapter in provider_registry() {
            let source = adapter.id();
            for entry in self.entries(&source) {
                let values = entry_values(adapter.as_ref(), &entry, &mut Vec::new());
                for key in adapter
                    .config_requirements()
                    .iter()
                    .filter(|key| key.is_secret())
                {
                    let Some(StoredValue::Text(text)) = values.get_stored(key.name) else {
                        continue;
                    };
                    let value = Binding::of(&source, &values)
                        .ok()
                        .and_then(|binding| sealer.seal(*key, &binding, text).ok());
                    let Some(value) = value else {
                        sealed.unbound.push((entry.id.clone(), *key));
                        continue;
                    };
                    let mut table = InlineTable::new();
                    table.insert(SEALED, Value::from(value.as_str()));
                    let field = field_name(&source, *key);
                    let target = self
                        .table_mut(&source, entry.index)
                        .expect("an entry just read names its table");
                    if let Some(old) = target.get(&field).and_then(Item::as_value) {
                        let decor = old.decor().clone();
                        *table.decor_mut() = decor;
                    }
                    target[field.as_str()] = Item::Value(Value::InlineTable(table));
                    sealed.count += 1;
                }
            }
        }
        sealed
    }

    /// The first connection whose sealed secret is as it was in `before` while what it is
    /// bound to changed: it would not open, so the change must bring the secret again. Bindings
    /// are compared, never their tags.
    pub fn rebound(&self, before: &ConfigFile) -> Option<(ConnectionId, ProviderConfigKey)> {
        let before = before.connections().connections;
        self.connections()
            .connections
            .into_iter()
            .find_map(|connection| {
                let old = before.iter().find(|old| old.id == connection.id)?;
                let source = connection.source().clone();
                let keys = adapter_of(&source).config_requirements();
                keys.iter()
                    .filter(|key| key.is_secret())
                    .find(|key| {
                        let stored = connection.values().get_stored(key.name);
                        matches!(stored, Some(StoredValue::Sealed(StoredSecret::Sealed(_))))
                            && stored == old.values().get_stored(key.name)
                            && Binding::of(&source, connection.values()).ok()
                                != Binding::of(&source, old.values()).ok()
                    })
                    .map(|key| (connection.id.clone(), *key))
            })
    }

    /// Opens every value `sealer`'s key sealed, for the binding its table names now: one that
    /// does not open must never be published.
    pub(in crate::settings) fn check_opens(
        &self,
        sealer: &Sealer,
    ) -> Result<(), (ConnectionId, ProviderConfigKey, Unopenable)> {
        for connection in self.connections().connections {
            let source = connection.source().clone();
            for key in adapter_of(&source)
                .config_requirements()
                .iter()
                .filter(|key| key.is_secret())
            {
                let Some(StoredValue::Sealed(StoredSecret::Sealed(value))) =
                    connection.values().get_stored(key.name)
                else {
                    continue;
                };
                if !sealer.sealed(value) {
                    continue;
                }
                let failed = |why| (connection.id.clone(), *key, why);
                let binding = Binding::of(&source, connection.values())
                    .map_err(|_| failed(Unopenable::OtherAddress))?;
                sealer.open(value, *key, &binding).map_err(failed)?;
            }
        }
        Ok(())
    }

    /// The secrets the file holds as plain text that `before` did not hold so (none, sealed,
    /// or other text): a new plain secret, typed into the file.
    pub fn new_plain_secrets(
        &self,
        before: Option<&ConfigFile>,
    ) -> Vec<(ConnectionId, ProviderConfigKey)> {
        let before = before
            .map(|file| file.connections().connections)
            .unwrap_or_default();
        self.plain_secrets()
            .into_iter()
            .filter(|(id, key)| {
                let old = before
                    .iter()
                    .find(|old| old.id == *id)
                    .and_then(|old| old.values().get_stored(key.name).cloned());
                let new = self
                    .connections()
                    .connections
                    .into_iter()
                    .find(|connection| connection.id == *id)
                    .and_then(|connection| connection.values().get_stored(key.name).cloned());
                old != new
            })
            .collect()
    }

    /// Every secret the file holds as plain text.
    pub fn plain_secrets(&self) -> Vec<(ConnectionId, ProviderConfigKey)> {
        self.connections()
            .connections
            .iter()
            .flat_map(|connection| {
                adapter_of(connection.source())
                    .config_requirements()
                    .iter()
                    .filter(|key| key.is_secret())
                    .filter(|key| {
                        matches!(
                            connection.values().get_stored(key.name),
                            Some(StoredValue::Text(_))
                        )
                    })
                    .map(|key| (connection.id.clone(), *key))
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// How many secrets the file holds sealed with `store`'s key, for the warning before a new
    /// key is made there.
    pub fn sealed_in(&self, store: crate::secrets::StoreKind) -> usize {
        self.connections()
            .connections
            .iter()
            .map(|connection| {
                connection
                    .values()
                    .sealed()
                    .filter(|(_, secret)| {
                        matches!(secret, StoredSecret::Sealed(value) if value.store() == store)
                    })
                    .count()
            })
            .sum()
    }
}
