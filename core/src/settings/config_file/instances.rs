use std::collections::{BTreeSet, HashMap};

use toml_edit::{Item, Table, value};

use super::{ConfigFile, edit::Refresh, read::Entry};
use crate::{
    model::ConnectionId,
    provider::provider_registry,
    settings::{ConfigEditError, INSTANCE},
};

impl ConfigFile {
    /// Gives each connection added or changed since the file was read an `instance` if it has
    /// none, or an unreadable one, and a new one where its lifetime changed. No other table is
    /// touched.
    pub(in crate::settings) fn assign_instances(&mut self) -> Result<(), ConfigEditError> {
        let touched = std::mem::take(&mut self.touched);
        let instances = self.instances();
        let mut used = instances
            .values()
            .filter_map(|read| match read {
                InstanceRead::Valid(instance) => Some(instance.clone()),
                InstanceRead::Malformed | InstanceRead::Repeated => None,
            })
            .collect::<BTreeSet<_>>();
        for (id, refresh) in touched {
            let entry = self
                .entries_with_warnings(&id.source, &instances, &mut Vec::new())
                .into_iter()
                .find(|entry| entry.id == id);
            let needs = |entry: &Entry| entry.instance.is_none() || refresh == Refresh::Always;
            if let Some(entry) = entry.filter(needs) {
                let instance = fresh_instance(&mut used);
                set_instance(self.table_mut(&id.source, entry.index)?, &instance);
            }
        }
        Ok(())
    }

    /// The `instance` of every table of every kind, by kind and index. A value that is not 16
    /// lowercase hex digits, or that a table earlier in the file already has, cannot be read.
    pub(in crate::settings) fn instances(&self) -> HashMap<(String, usize), InstanceRead> {
        let mut seen = BTreeSet::new();
        let mut instances = HashMap::new();
        for (kind, index, table) in self.tables_in_file_order() {
            let Some(instance) = table.get(INSTANCE) else {
                continue;
            };
            let read = match instance.as_str().filter(|text| valid_instance(text)) {
                None => InstanceRead::Malformed,
                Some(text) if !seen.insert(text.to_string()) => InstanceRead::Repeated,
                Some(text) => InstanceRead::Valid(text.to_string()),
            };
            instances.insert((kind.to_string(), index), read);
        }
        instances
    }

    /// Every connection table with its kind and index, in the order they appear in the file;
    /// tables added since it was read come last.
    pub(in crate::settings) fn tables_in_file_order(&self) -> Vec<(&str, usize, &Table)> {
        let kinds = provider_registry()
            .iter()
            .map(|adapter| adapter.id().as_ref().to_string())
            .collect::<BTreeSet<_>>();
        let mut tables = self
            .doc
            .iter()
            .filter(|(kind, _)| kinds.contains(*kind))
            .filter_map(|(kind, item)| Some((kind, item.as_array_of_tables()?)))
            .flat_map(|(kind, tables)| {
                tables
                    .iter()
                    .enumerate()
                    .map(move |(index, table)| (kind, index, table))
            })
            .collect::<Vec<_>>();
        tables.sort_by_key(|(_, _, table)| table.position().unwrap_or(isize::MAX));
        tables
    }

    /// What each table of every kind is: the connection it is read as (none when skipped) and
    /// its readable instance.
    pub(super) fn lifetimes(&self) -> HashMap<(String, usize), Lifetime> {
        let instances = self.instances();
        let mut lifetimes = HashMap::new();
        for (kind, index, _) in self.tables_in_file_order() {
            lifetimes.insert(
                (kind.to_string(), index),
                Lifetime {
                    id: None,
                    instance: None,
                },
            );
        }
        for adapter in provider_registry() {
            let source = adapter.id();
            for entry in self.entries_with_warnings(&source, &instances, &mut Vec::new()) {
                lifetimes.insert(
                    (source.as_ref().to_string(), entry.index),
                    Lifetime {
                        id: Some(entry.id),
                        instance: entry.instance,
                    },
                );
            }
        }
        lifetimes
    }
}

/// What a table is read as.
pub(super) struct Lifetime {
    pub(super) id: Option<ConnectionId>,
    pub(super) instance: Option<String>,
}

/// Writes `instance` as a plain `instance = "…"`; a table under that name (`instance.x = …`)
/// goes, key and all, since the name is reserved.
pub(in crate::settings) fn set_instance(table: &mut Table, instance: &str) {
    if !matches!(table.get(INSTANCE), None | Some(Item::Value(_))) {
        table.remove(INSTANCE);
    }
    table[INSTANCE] = value(instance);
}

/// A table's `instance` as read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::settings) enum InstanceRead {
    Valid(String),
    Malformed,
    /// An earlier table already has it.
    Repeated,
}

pub(in crate::settings) fn valid_instance(text: &str) -> bool {
    text.len() == 16
        && text
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

/// A new instance that is not in `used`, which then holds it.
pub(in crate::settings) fn fresh_instance(used: &mut BTreeSet<String>) -> String {
    loop {
        let instance = format!("{:016x}", crate::utils::random_u64());
        if used.insert(instance.clone()) {
            return instance;
        }
    }
}
