use std::collections::{BTreeMap, BTreeSet};

use toml_edit::{ArrayOfTables, Item, Table, Value, value};

use super::{
    ConfigFile,
    read::{Entry, base_label, entry_values, numbered, raw_value},
};
use crate::{
    model::{ConnectionId, Source},
    provider::{
        adapter as adapter_of,
        connection::{ConnectionLocation, Instances, slug},
        provider_config::{
            ConfigSource, ProviderConfigKey, ProviderConfigShape, ProviderConfigValueType,
            parse_config_list,
        },
    },
    settings::{ConfigEditError, EditBase, FieldId, LABEL, NAME, WSL, field_name},
};

impl ConfigFile {
    /// Adds a connection of `source` and returns its id, which comes from the label. Without a
    /// label it gets the kind's name (`GitLab`, `GitLab 2`), or its location for Git.
    pub fn add(
        &mut self,
        source: &Source,
        label: Option<&str>,
        location: ConnectionLocation,
    ) -> Result<ConnectionId, ConfigEditError> {
        let adapter = adapter_of(source);
        let existing = self.entries(source);
        if adapter.instances() == Instances::Single && !existing.is_empty() {
            return Err(ConfigEditError::SingleInstance(source.clone()));
        }
        if matches!(location, ConnectionLocation::Wsl(_)) && *source != Source::Git {
            return Err(ConfigEditError::WslOnlyForGit);
        }
        let label = match label.map(str::trim) {
            Some("") => return Err(ConfigEditError::EmptyLabel),
            Some(label) => label.to_string(),
            None => {
                let taken = existing
                    .iter()
                    .map(|entry| entry.label.clone())
                    .collect::<BTreeSet<_>>();
                numbered(&base_label(source, &location), &taken, " ")
            }
        };
        // A name no connection has: it changes no other table's id, so none is pinned.
        let taken = existing
            .into_iter()
            .map(|entry| entry.id.name)
            .collect::<BTreeSet<_>>();
        let name = numbered(&slug(&label), &taken, "-");

        let mut table = Table::new();
        table.insert(NAME, value(name.as_str()));
        table.insert(LABEL, value(label.as_str()));
        if let ConnectionLocation::Wsl(distro) = &location {
            table.insert(WSL, value(distro.as_str()));
        }
        self.tables_mut(source)?.push(table);
        let id = ConnectionId::new(source.clone(), name);
        self.touched.insert(id.clone(), Refresh::IfMissing);
        Ok(id)
    }

    pub fn remove(&mut self, id: &ConnectionId) -> Result<(), ConfigEditError> {
        let index = self.index_of(id)?;
        let before = self.lifetimes();
        let tables = self.tables_mut(&id.source)?;
        tables.remove(index);
        if tables.is_empty() {
            self.doc.remove(id.source.as_ref());
        }
        self.touched.remove(id);
        let kind = id.source.as_ref().to_string();
        let was = |table_kind: &str, at: usize| {
            let at = if table_kind == kind && at >= index {
                at + 1
            } else {
                at
            };
            before.get(&(table_kind.to_string(), at))
        };

        // Ids that come from labels and order may move up; those tables keep theirs. Writing a
        // name can move another one, so this runs until nothing moves.
        let tables = self.entries(&id.source).len();
        for _ in 0..=tables {
            let moved = self
                .entries(&id.source)
                .into_iter()
                .filter_map(|entry| {
                    let old = was(&kind, entry.index)?.id.clone()?;
                    (old != entry.id).then_some((entry, old))
                })
                .collect::<Vec<_>>();
            if moved.is_empty() {
                break;
            }
            for (entry, old) in moved {
                self.table_mut(&id.source, entry.index)?[NAME] = value(old.name.as_str());
                self.touched.entry(old).or_insert(Refresh::IfMissing);
            }
        }

        // A table that was skipped and is now read (a second Outlook table), or whose instance
        // only now reads as its own (a repeat of the removed one's), is a new lifetime: a writer
        // that saw the removed connection must not take it for that one.
        for ((table_kind, at), now) in self.lifetimes() {
            let Some(now_id) = now.id else {
                continue;
            };
            let was = was(&table_kind, at);
            let promoted = was.is_none_or(|was| was.id.is_none());
            let newly_read =
                now.instance.is_some() && was.is_some_and(|was| was.instance.is_none());
            if promoted || newly_read {
                self.touched.insert(now_id, Refresh::Always);
            }
        }
        Ok(())
    }

    /// Refuses an id no connection of the file has, for an edit that may change nothing.
    pub fn require(&self, id: &ConnectionId) -> Result<(), ConfigEditError> {
        self.entry(id).map(|_| ())
    }

    pub fn rename(&mut self, id: &ConnectionId, label: &str) -> Result<(), ConfigEditError> {
        let label = label.trim();
        if label.is_empty() {
            return Err(ConfigEditError::EmptyLabel);
        }
        // The label it already shows: nothing to write, not even its name.
        if self.entry(id)?.label == label {
            return Ok(());
        }
        let (index, before) = self.pin(id)?;
        let table = self.table_mut(&id.source, index)?;
        if table.get(LABEL).and_then(Item::as_str) != Some(label) {
            assign(table, LABEL, value(label));
        }
        self.mark_if_changed(id, index, &before)
    }

    /// Sets `key` of a connection to `raw` (validated by the caller), or removes it with `None`
    /// so the default applies again.
    pub fn set(
        &mut self,
        id: &ConnectionId,
        key: ProviderConfigKey,
        raw: Option<&str>,
    ) -> Result<(), ConfigEditError> {
        let adapter = adapter_of(&id.source);
        if !adapter.config_requirements().contains(&key) {
            return Err(ConfigEditError::UnknownSetting {
                source: id.source.clone(),
                key: key.name,
            });
        }
        let field = field_name(&id.source, key);
        let item = raw.map(|raw| typed_item(key, raw));
        // The value it already has, however it is spelled: nothing to write, not even its name.
        let current = self.entry(id)?.table.get(&field).map(raw_value);
        let unchanged = match &item {
            Some(item) => current == Some(raw_value(item)),
            None => current.is_none(),
        };
        if unchanged {
            return Ok(());
        }
        let (index, before) = self.pin(id)?;
        let table = self.table_mut(&id.source, index)?;
        match item {
            Some(item) => assign(table, &field, item),
            None => {
                table.remove(&field);
            }
        }
        self.mark_if_changed(id, index, &before)
    }

    /// What a writer sees of `id` before it changes the `touched` fields: those fields, the
    /// kind's account fields and the connection's instance. `None` for an unknown connection.
    pub fn edit_base(&self, id: &ConnectionId, touched: &[FieldId]) -> Option<EditBase> {
        let adapter = adapter_of(&id.source);
        let account = adapter
            .config_requirements()
            .iter()
            .filter(|key| key.account)
            .map(|key| FieldId::Value(key.name));
        let fields = touched.iter().cloned().chain(account).collect::<Vec<_>>();
        let seen = self.seen(id, &fields)?;
        Some(EditBase::new(
            id.clone(),
            seen.label,
            seen.instance,
            seen.fields,
        ))
    }

    /// The instance of `id` and the values of `fields`, as preconditions compare them.
    pub(in crate::settings) fn seen(&self, id: &ConnectionId, fields: &[FieldId]) -> Option<Seen> {
        let adapter = adapter_of(&id.source);
        let entry = self
            .entries(&id.source)
            .into_iter()
            .find(|entry| entry.id == *id)?;
        let values = entry_values(adapter.as_ref(), &entry, &mut Vec::new());
        let fields = fields
            .iter()
            .map(|field| {
                let seen = match field {
                    FieldId::Label => Some(entry.label.clone()),
                    FieldId::Value(key) => values.get(key),
                };
                (field.clone(), seen)
            })
            .collect();
        Some(Seen {
            label: entry.label,
            instance: entry.instance,
            fields,
        })
    }

    /// Writes a connection's name and label before it is edited, so its id stays as it was, and
    /// returns its index with the table's text as it was. The other tables keep their ids
    /// without it.
    fn pin(&mut self, id: &ConnectionId) -> Result<(usize, String), ConfigEditError> {
        let entry = self.entry(id)?;
        let before = entry.table.to_string();
        self.pin_entry(&entry)?;
        Ok((entry.index, before))
    }

    /// Counts `id` as written when its table differs from `before`, so an edit that sets what
    /// is already there changes nothing.
    fn mark_if_changed(
        &mut self,
        id: &ConnectionId,
        index: usize,
        before: &str,
    ) -> Result<(), ConfigEditError> {
        if self.table_mut(&id.source, index)?.to_string() != before {
            self.touched.entry(id.clone()).or_insert(Refresh::IfMissing);
        }
        Ok(())
    }

    fn entry(&self, id: &ConnectionId) -> Result<Entry, ConfigEditError> {
        self.entries(&id.source)
            .into_iter()
            .find(|entry| entry.id == *id)
            .ok_or_else(|| ConfigEditError::UnknownConnection(id.clone()))
    }

    fn pin_entry(&mut self, entry: &Entry) -> Result<(), ConfigEditError> {
        let table = self.table_mut(&entry.id.source, entry.index)?;
        // Indexing keeps a key's comments, unlike `insert`.
        if table.get(NAME).and_then(Item::as_str) != Some(entry.id.name.as_str()) {
            table[NAME] = value(entry.id.name.as_str());
        }
        if table.get(LABEL).and_then(Item::as_str) != Some(entry.label.as_str()) {
            table[LABEL] = value(entry.label.as_str());
        }
        Ok(())
    }

    fn index_of(&self, id: &ConnectionId) -> Result<usize, ConfigEditError> {
        self.entries(&id.source)
            .iter()
            .find(|entry| entry.id == *id)
            .map(|entry| entry.index)
            .ok_or_else(|| ConfigEditError::UnknownConnection(id.clone()))
    }

    fn tables_mut(&mut self, source: &Source) -> Result<&mut ArrayOfTables, ConfigEditError> {
        let item = self
            .doc
            .entry(source.as_ref())
            .or_insert(Item::ArrayOfTables(ArrayOfTables::new()));
        if matches!(item, Item::Value(Value::Array(array)) if array.is_empty()) {
            *item = Item::ArrayOfTables(ArrayOfTables::new());
        }
        item.as_array_of_tables_mut()
            .ok_or_else(|| ConfigEditError::NotATableList(source.clone()))
    }

    /// The table at `index`, which an entry of this file just read.
    pub(super) fn table_mut(
        &mut self,
        source: &Source,
        index: usize,
    ) -> Result<&mut Table, ConfigEditError> {
        Ok(self
            .tables_mut(source)?
            .get_mut(index)
            .expect("an entry's index names a table of its kind"))
    }
}

/// Whether a written table needs a new instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Refresh {
    /// Only if it has none it can read.
    IfMissing,
    /// Always: it became another lifetime.
    Always,
}

/// Replaces a value, keeping the comment and spacing around the old one.
fn assign(table: &mut Table, key: &str, mut item: Item) {
    if let (Some(old), Some(new)) = (table.get(key).and_then(Item::as_value), item.as_value_mut()) {
        *new.decor_mut() = old.decor().clone();
    }
    table[key] = item;
}

/// What a writer saw of one connection.
pub(in crate::settings) struct Seen {
    label: String,
    pub(in crate::settings) instance: Option<String>,
    pub(in crate::settings) fields: BTreeMap<FieldId, Option<String>>,
}

/// Writes a value with its TOML type: `true`, `42`, `["a", "b"]` or text.
fn typed_item(key: ProviderConfigKey, raw: &str) -> Item {
    match key.shape {
        ProviderConfigShape::List(_) => Item::Value(Value::Array(
            parse_config_list(raw).iter().map(String::as_str).collect(),
        )),
        ProviderConfigShape::Single(ProviderConfigValueType::Bool) => {
            match raw.trim().to_ascii_lowercase().as_str() {
                "true" => value(true),
                "false" => value(false),
                _ => value(raw),
            }
        }
        ProviderConfigShape::Single(ProviderConfigValueType::Int) => {
            match raw.trim().parse::<i64>() {
                Ok(number) => value(number),
                Err(_) => value(raw),
            }
        }
        ProviderConfigShape::Single(_) => value(raw),
    }
}
