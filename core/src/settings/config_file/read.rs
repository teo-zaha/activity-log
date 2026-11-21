use std::collections::{BTreeMap, BTreeSet, HashMap};

use toml_edit::{Item, Table, Value};

use super::{ConfigFile, ResolvedConnections, instances::InstanceRead};
use crate::{
    model::{ConnectionId, Source},
    provider::{
        adapter as adapter_of,
        connection::{Connection, ConnectionLocation, ConnectionValues, Instances, slug},
        git::{LOCAL_LABEL, wsl_label},
        provider_adapter::ProviderAdapter,
        provider_config::encode_config_list,
        provider_registry,
    },
    settings::{INSTANCE, LABEL, NAME, WSL, field_name},
};

impl ConfigFile {
    /// Every connection, with the problems in single connections logged instead of returned.
    pub fn logged_connections(&self) -> Vec<Connection> {
        let resolved = self.connections();
        for warning in resolved.warnings {
            crate::diagnostics::warn("config", warning);
        }
        resolved.connections
    }

    /// Every connection of every provider kind, in the file's order. Tables that cannot be read
    /// are skipped with a warning.
    pub fn connections(&self) -> ResolvedConnections {
        let mut resolved = ResolvedConnections {
            connections: Vec::new(),
            warnings: Vec::new(),
        };
        let instances = self.instances();
        for adapter in provider_registry() {
            let entries =
                self.entries_with_warnings(&adapter.id(), &instances, &mut resolved.warnings);
            for entry in entries {
                let values = entry_values(adapter.as_ref(), &entry, &mut resolved.warnings);
                resolved.connections.push(
                    Connection::new(entry.id, entry.label, entry.location, values)
                        .with_instance(entry.instance),
                );
            }
        }
        resolved
    }

    /// The connections of a kind as written in the file.
    pub(in crate::settings) fn entries(&self, source: &Source) -> Vec<Entry> {
        self.entries_with_warnings(source, &self.instances(), &mut Vec::new())
    }

    /// The connections of a kind with unique ids. Names are compared as slugs, explicit names
    /// first so a table without one never takes them; a repeated name is numbered (`work-2`) in
    /// file order and reported, rather than failing every connection. The next edit writes the
    /// numbered name into the file. A table that cannot be read is skipped with a warning, and
    /// each entry keeps its index in the file so edits address the right table.
    pub(in crate::settings) fn entries_with_warnings(
        &self,
        source: &Source,
        instances: &HashMap<(String, usize), InstanceRead>,
        warnings: &mut Vec<String>,
    ) -> Vec<Entry> {
        let Some(item) = self.doc.get(source.as_ref()) else {
            return Vec::new();
        };
        let tables: Vec<(usize, &Table)> = match item {
            Item::ArrayOfTables(tables) => tables.iter().enumerate().collect(),
            Item::Value(Value::Array(array)) if array.is_empty() => Vec::new(),
            _ => {
                warnings.push(format!(
                    "`{}` must be a list of [[{}]] tables; it is skipped",
                    source.as_ref(),
                    source.as_ref()
                ));
                return Vec::new();
            }
        };
        let single = adapter_of(source).instances() == Instances::Single;
        let mut readable = Vec::new();
        for (index, table) in tables {
            if *source != Source::Git && table.contains_key(WSL) {
                warnings.push(format!(
                    "{source}: only [[git]] connections can set `wsl`; connection {} is skipped",
                    index + 1
                ));
            } else if single && !readable.is_empty() {
                warnings.push(format!(
                    "{source}: there can be only one connection; connection {} is skipped",
                    index + 1
                ));
            } else {
                readable.push((index, table));
            }
        }

        let explicit = readable
            .iter()
            .map(|(_, table)| table.get(NAME).and_then(Item::as_str).map(slug))
            .collect::<Vec<_>>();
        let mut taken = BTreeSet::new();
        let first_claims = explicit
            .iter()
            .map(|name| name.as_ref().is_some_and(|name| taken.insert(name.clone())))
            .collect::<Vec<_>>();

        let mut entries = Vec::new();
        for (((index, table), explicit), first_claim) in
            readable.into_iter().zip(explicit).zip(first_claims)
        {
            let location = match table.get(WSL).and_then(Item::as_str) {
                Some(distro) => ConnectionLocation::Wsl(distro.to_string()),
                None => ConnectionLocation::Local,
            };
            let label = table
                .get(LABEL)
                .and_then(Item::as_str)
                .map(str::trim)
                .filter(|label| !label.is_empty())
                .map_or_else(|| base_label(source, &location), str::to_string);
            let name = match explicit {
                Some(name) if first_claim => name,
                Some(name) => {
                    let numbered = numbered(&name, &taken, "-");
                    warnings.push(format!(
                        "{source}: more than one connection is named `{name}`; \
                         the one labelled \"{label}\" is read as `{numbered}`"
                    ));
                    taken.insert(numbered.clone());
                    numbered
                }
                None => {
                    let name = numbered(&slug(&label), &taken, "-");
                    taken.insert(name.clone());
                    name
                }
            };
            let id = ConnectionId::new(source.clone(), name);
            let instance = match instances.get(&(source.as_ref().to_string(), index)) {
                Some(InstanceRead::Valid(instance)) => Some(instance.clone()),
                Some(InstanceRead::Malformed) => {
                    warnings.push(format!(
                        "{id}: `instance` is not 16 lowercase hex digits; it gets a new one \
                         when Activity Log next saves this connection"
                    ));
                    None
                }
                Some(InstanceRead::Repeated) => {
                    warnings.push(format!(
                        "{id}: `instance` repeats another connection's; it gets a new one \
                         when Activity Log next saves this connection"
                    ));
                    None
                }
                None => None,
            };
            entries.push(Entry {
                id,
                index,
                label,
                location,
                instance,
                table: table.clone(),
            });
        }
        entries
    }
}

/// One connection as found in the file.
pub(in crate::settings) struct Entry {
    pub(in crate::settings) id: ConnectionId,
    /// Its position in the kind's list of tables, skipped tables included.
    pub(in crate::settings) index: usize,
    pub(super) label: String,
    location: ConnectionLocation,
    /// `None` when the table has none or one that cannot be read.
    pub(in crate::settings) instance: Option<String>,
    pub(in crate::settings) table: Table,
}

/// The values a connection's table sets, by config key. Unknown fields are reported, not fatal,
/// so one typo does not take every connection down.
pub(super) fn entry_values(
    adapter: &dyn ProviderAdapter,
    entry: &Entry,
    warnings: &mut Vec<String>,
) -> ConnectionValues {
    let source = adapter.id();
    let keys = adapter
        .config_requirements()
        .iter()
        .map(|key| (field_name(&source, *key), *key))
        .collect::<BTreeMap<_, _>>();
    let mut values = BTreeMap::new();
    for (field, item) in entry.table.iter() {
        if matches!(field, NAME | LABEL | WSL | INSTANCE) {
            continue;
        }
        let Some(key) = keys.get(field) else {
            warnings.push(format!("{}: unknown setting `{field}`", entry.id));
            continue;
        };
        match raw_value(item) {
            Some(raw) => {
                let raw = (!raw.is_empty()).then_some(raw);
                values.insert(key.name.to_string(), raw);
            }
            None => warnings.push(format!(
                "{}: `{field}` must be text, a number, true/false or a list of text",
                entry.id
            )),
        }
    }
    ConnectionValues::new(values)
}

/// A field as the config machinery reads it: lists as their JSON form.
pub(super) fn raw_value(item: &Item) -> Option<String> {
    let value = item.as_value()?;
    match value {
        Value::String(text) => Some(text.value().to_string()),
        Value::Boolean(flag) => Some(flag.value().to_string()),
        Value::Integer(number) => Some(number.value().to_string()),
        Value::Array(array) => array
            .iter()
            .map(|item| item.as_str().map(str::to_string))
            .collect::<Option<Vec<_>>>()
            .map(|items| encode_config_list(&items)),
        _ => None,
    }
}

/// A connection's label when none is given: where Git reads, else the kind's name.
pub(super) fn base_label(source: &Source, location: &ConnectionLocation) -> String {
    match (source, location) {
        (Source::Git, ConnectionLocation::Local) => LOCAL_LABEL.to_string(),
        (Source::Git, ConnectionLocation::Wsl(distro)) => wsl_label(distro),
        _ => source.label().to_string(),
    }
}

/// `wanted`, or `wanted` with the first free number from 2 after `separator`.
pub(super) fn numbered(wanted: &str, taken: &BTreeSet<String>, separator: &str) -> String {
    if !taken.contains(wanted) {
        return wanted.to_string();
    }
    (2..)
        .map(|n| format!("{wanted}{separator}{n}"))
        .find(|name| !taken.contains(name))
        .expect("an unused name exists")
}
