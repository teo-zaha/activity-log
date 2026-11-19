//! `config.toml`: the connections of every provider, shared by the CLI, the API server and the
//! desktop app. Each provider kind is an array of tables, one per connection:
//!
//! ```toml
//! [[gitlab]]
//! name = "work"
//! label = "Work GitLab"
//! url = "https://gitlab.example.com"
//! token = "glpat-…"
//! ```
//!
//! Field names are the config keys without the kind prefix, lowercased (`GITLAB_URL` → `url`).
//! The file is the only list of sources: a kind it does not mention has none. Every writer goes
//! through [`edit_config`] (or [`edit_externally`] for `config edit`); readers never lock or
//! write.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    env, fs, io,
    path::{Path, PathBuf},
    sync::{Arc, Once},
};

use anyhow::{Context, Result, anyhow, bail};
use toml_edit::{ArrayOfTables, DocumentMut, Item, Table, Value, value};

use crate::{
    model::{ConnectionId, Source},
    platform::private_file::readable_by_others,
    provider::{
        connection::{Connection, ConnectionLocation, ConnectionValues, Instances, slug},
        git::{LOCAL_LABEL, wsl_label},
        provider_adapter::ProviderAdapter,
        provider_config::{
            ConfigSource, ProviderConfigKey, ProviderConfigShape, ProviderConfigValueType,
            encode_config_list, parse_config_list,
        },
        provider_registry,
    },
};

mod external;
mod precondition;
mod write;

pub use external::{ExternalEdit, ExternalEditError, edit_externally, editor_command};
pub use precondition::{Conflict, ConflictKind, EditBase, FieldId, Precondition};
pub(crate) use write::lock_wait;
pub use write::{EditError, Edited, edit_config};

#[cfg(test)]
mod tests;

const NAME: &str = "name";
const LABEL: &str = "label";
const WSL: &str = "wsl";
/// A reserved, non-secret id that tells two lifetimes of a connection with the same name apart.
/// Written into a table when the app first writes that table.
const INSTANCE: &str = "instance";

/// What every reader and writer says when the OS gives no folder for the settings.
pub const NO_LOCATION: &str = "Activity Log has no folder for its settings. Set ACTIVITY_LOG_CONFIG to a file path, or start it with --config <path>.";

/// Logs [`NO_LOCATION`] once per process.
pub fn warn_no_location() {
    static WARNED: Once = Once::new();
    WARNED.call_once(|| crate::diagnostics::warn("config", NO_LOCATION));
}

/// The connections from the config file at `explicit` (or the default path). Problems in single
/// connections are logged, not returned. Without a settings location there are none.
pub fn load_connections(explicit: Option<&Path>) -> Result<Vec<Connection>> {
    match ConfigFile::resolve_path(explicit) {
        Some(path) => ConfigFile::load(path)?.logged_connections(),
        None => {
            warn_no_location();
            Ok(Vec::new())
        }
    }
}

/// The bytes of the config file, `None` when it does not exist. The first file read in a
/// process that other accounts can read is reported once.
pub fn read_config_bytes(path: &Path) -> io::Result<Option<Vec<u8>>> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err),
    };
    warn_if_shared(path);
    Ok(Some(bytes))
}

/// Reports, once per process, a settings file other accounts can read.
fn warn_if_shared(path: &Path) {
    static CHECKED: Once = Once::new();
    CHECKED.call_once(|| {
        if readable_by_others(path).unwrap_or(false) {
            let name = path.file_name().unwrap_or(path.as_os_str()).to_string_lossy();
            crate::diagnostics::warn(
                "config",
                format!(
                    "{name} can be read by other accounts; Activity Log makes it private the next time it saves it"
                ),
            );
        }
    });
}

/// A stable FNV-1a hash of config bytes; a missing file has the id of empty bytes. Used only
/// inside one process, never sent to a client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContentId(u64);

impl ContentId {
    pub fn of(bytes: &[u8]) -> Self {
        Self(crate::utils::fnv1a(bytes.iter().copied()))
    }
}

/// A config file that is not valid UTF-8 or TOML. It names the place and the parser's short
/// message only, and has no source: no part of the file's text (a token on the bad line) can
/// reach a log through it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    pub path: PathBuf,
    pub line: usize,
    pub column: usize,
    pub message: String,
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} is not valid TOML (line {}, column {}): {}",
            self.path.display(),
            self.line,
            self.column,
            self.message
        )
    }
}

impl std::error::Error for ConfigError {}

/// The connections file and its path.
#[derive(Debug, Clone)]
pub struct ConfigFile {
    path: PathBuf,
    doc: DocumentMut,
    /// Connections added or changed since the file was read, and whether they need a new
    /// `instance` even if they have one; they get it when the file is written.
    touched: BTreeMap<ConnectionId, Refresh>,
}

/// Connections read from the file, with problems that did not stop reading it.
#[derive(Debug, Clone)]
pub struct ResolvedConnections {
    pub connections: Vec<Connection>,
    pub warnings: Vec<String>,
}

impl ConfigFile {
    /// `explicit`, else `ACTIVITY_LOG_CONFIG`, else `activity-log/config.toml` in the OS config
    /// directory, as an absolute path. `None` when there is neither a path nor an OS config
    /// directory: the settings then have no location (see [`NO_LOCATION`]).
    pub fn resolve_path(explicit: Option<&Path>) -> Option<PathBuf> {
        let path = explicit
            .map(Path::to_path_buf)
            .or_else(|| env_path("ACTIVITY_LOG_CONFIG"))
            .or_else(|| config_dir().map(|dir| dir.join("activity-log").join("config.toml")))?;
        Some(std::path::absolute(&path).unwrap_or(path))
    }

    /// Reads the file; a missing file is an empty configuration.
    pub fn load(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let bytes = read_config_bytes(&path)
            .with_context(|| format!("Reading {}", path.display()))?
            .unwrap_or_default();
        Ok(Self::parse(path, &bytes)?)
    }

    /// Parses `bytes`; invalid UTF-8 and invalid TOML both fail with a [`ConfigError`].
    pub fn parse(path: impl Into<PathBuf>, bytes: &[u8]) -> Result<Self, ConfigError> {
        let path = path.into();
        let error = |offset: usize, message: String| {
            let (line, column) = position(bytes, offset);
            ConfigError {
                path: path.clone(),
                line,
                column,
                message,
            }
        };
        let text = std::str::from_utf8(bytes)
            .map_err(|err| error(err.valid_up_to(), "invalid UTF-8".to_string()))?;
        let doc = text.parse::<DocumentMut>().map_err(|err| {
            let offset = err.span().map_or(0, |span| span.start);
            error(offset, err.message().to_string())
        })?;
        Ok(Self {
            path,
            doc,
            touched: BTreeMap::new(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Every connection, with the problems in single connections logged instead of returned.
    pub fn logged_connections(&self) -> Result<Vec<Connection>> {
        let resolved = self.connections()?;
        for warning in resolved.warnings {
            crate::diagnostics::warn("config", warning);
        }
        Ok(resolved.connections)
    }

    /// Every connection of every provider kind, in the file's order. Tables that cannot be read
    /// are skipped with a warning.
    pub fn connections(&self) -> Result<ResolvedConnections> {
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
                resolved.connections.push(Connection::new(
                    entry.id,
                    entry.label,
                    entry.location,
                    values,
                ));
            }
        }
        Ok(resolved)
    }

    /// Adds a connection of `source` and returns its id, which comes from the label. Without a
    /// label it gets the kind's name (`GitLab`, `GitLab 2`), or its location for Git.
    pub fn add(
        &mut self,
        source: &Source,
        label: Option<&str>,
        location: ConnectionLocation,
    ) -> Result<ConnectionId> {
        let adapter = adapter_for(source)?;
        let existing = self.entries(source);
        if adapter.instances() == Instances::Single && !existing.is_empty() {
            bail!("{} is already added", source.label());
        }
        if matches!(location, ConnectionLocation::Wsl(_)) && *source != Source::Git {
            bail!("Only Git connections can be in WSL");
        }
        let label = match label.map(str::trim) {
            Some("") => bail!("A connection needs a label"),
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

    pub fn remove(&mut self, id: &ConnectionId) -> Result<()> {
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

    pub fn rename(&mut self, id: &ConnectionId, label: &str) -> Result<()> {
        let label = label.trim();
        if label.is_empty() {
            bail!("A connection needs a label");
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
    ) -> Result<()> {
        let adapter = adapter_for(&id.source)?;
        if !adapter.config_requirements().contains(&key) {
            bail!("{} has no setting {}", id.source.label(), key.name);
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
        let adapter = adapter_for(&id.source).ok()?;
        let account = adapter
            .config_requirements()
            .iter()
            .filter(|key| key.account)
            .map(|key| FieldId::Value(key.name));
        let fields = touched.iter().cloned().chain(account).collect::<Vec<_>>();
        let seen = self.seen(id, &fields)?;
        Some(EditBase::new(id.clone(), seen.instance, seen.fields))
    }

    /// The instance of `id` and the values of `fields`, as preconditions compare them.
    fn seen(&self, id: &ConnectionId, fields: &[FieldId]) -> Option<Seen> {
        let adapter = adapter_for(&id.source).ok()?;
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
            instance: entry.instance,
            fields,
        })
    }

    /// Gives each connection added or changed since the file was read an `instance` if it has
    /// none, or an unreadable one, and a new one where its lifetime changed. No other table is
    /// touched.
    fn assign_instances(&mut self) -> Result<()> {
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

    /// The connections of a kind as written in the file.
    fn entries(&self, source: &Source) -> Vec<Entry> {
        self.entries_with_warnings(source, &self.instances(), &mut Vec::new())
    }

    /// The connections of a kind with unique ids. Names are compared as slugs, explicit names
    /// first so a table without one never takes them; a repeated name is numbered (`work-2`) in
    /// file order and reported, rather than failing every connection. The next edit writes the
    /// numbered name into the file. A table that cannot be read is skipped with a warning, and
    /// each entry keeps its index in the file so edits address the right table.
    fn entries_with_warnings(
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
        let single =
            adapter_for(source).is_ok_and(|adapter| adapter.instances() == Instances::Single);
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

    /// The `instance` of every table of every kind, by kind and index. A value that is not 16
    /// lowercase hex digits, or that a table earlier in the file already has, cannot be read.
    fn instances(&self) -> HashMap<(String, usize), InstanceRead> {
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
    fn tables_in_file_order(&self) -> Vec<(&str, usize, &Table)> {
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
    fn lifetimes(&self) -> HashMap<(String, usize), Lifetime> {
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

    /// Writes a connection's name and label before it is edited, so its id stays as it was, and
    /// returns its index with the table's text as it was. The other tables keep their ids
    /// without it.
    fn pin(&mut self, id: &ConnectionId) -> Result<(usize, String)> {
        let entry = self.entry(id)?;
        let before = entry.table.to_string();
        self.pin_entry(&entry)?;
        Ok((entry.index, before))
    }

    /// Counts `id` as written when its table differs from `before`, so an edit that sets what
    /// is already there changes nothing.
    fn mark_if_changed(&mut self, id: &ConnectionId, index: usize, before: &str) -> Result<()> {
        if self.table_mut(&id.source, index)?.to_string() != before {
            self.touched.entry(id.clone()).or_insert(Refresh::IfMissing);
        }
        Ok(())
    }

    fn entry(&self, id: &ConnectionId) -> Result<Entry> {
        self.entries(&id.source)
            .into_iter()
            .find(|entry| entry.id == *id)
            .ok_or_else(|| anyhow!("No connection {id}"))
    }

    fn pin_entry(&mut self, entry: &Entry) -> Result<()> {
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

    fn index_of(&self, id: &ConnectionId) -> Result<usize> {
        self.entries(&id.source)
            .iter()
            .find(|entry| entry.id == *id)
            .map(|entry| entry.index)
            .ok_or_else(|| anyhow!("No connection {id}"))
    }

    fn tables_mut(&mut self, source: &Source) -> Result<&mut ArrayOfTables> {
        let item = self
            .doc
            .entry(source.as_ref())
            .or_insert(Item::ArrayOfTables(ArrayOfTables::new()));
        if matches!(item, Item::Value(Value::Array(array)) if array.is_empty()) {
            *item = Item::ArrayOfTables(ArrayOfTables::new());
        }
        item.as_array_of_tables_mut()
            .ok_or_else(|| anyhow!("`{}` must be a list of tables", source.as_ref()))
    }

    fn table_mut(&mut self, source: &Source, index: usize) -> Result<&mut Table> {
        self.tables_mut(source)?
            .get_mut(index)
            .ok_or_else(|| anyhow!("Connection {index} of {} is gone", source.as_ref()))
    }
}

impl std::fmt::Display for ConfigFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.doc)
    }
}

/// One connection as found in the file.
struct Entry {
    id: ConnectionId,
    /// Its position in the kind's list of tables, skipped tables included.
    index: usize,
    label: String,
    location: ConnectionLocation,
    /// `None` when the table has none or one that cannot be read.
    instance: Option<String>,
    table: Table,
}

/// Whether a written table needs a new instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Refresh {
    /// Only if it has none it can read.
    IfMissing,
    /// Always: it became another lifetime.
    Always,
}

/// What a table is read as.
struct Lifetime {
    id: Option<ConnectionId>,
    instance: Option<String>,
}

/// Writes `instance` as a plain `instance = "…"`; a table under that name (`instance.x = …`)
/// goes, key and all, since the name is reserved.
fn set_instance(table: &mut Table, instance: &str) {
    if !matches!(table.get(INSTANCE), None | Some(Item::Value(_))) {
        table.remove(INSTANCE);
    }
    table[INSTANCE] = value(instance);
}

/// Replaces a value, keeping the comment and spacing around the old one.
fn assign(table: &mut Table, key: &str, mut item: Item) {
    if let (Some(old), Some(new)) = (table.get(key).and_then(Item::as_value), item.as_value_mut()) {
        *new.decor_mut() = old.decor().clone();
    }
    table[key] = item;
}

/// What a writer saw of one connection.
struct Seen {
    instance: Option<String>,
    fields: BTreeMap<FieldId, Option<String>>,
}

/// A table's `instance` as read.
#[derive(Debug, Clone, PartialEq, Eq)]
enum InstanceRead {
    Valid(String),
    Malformed,
    /// An earlier table already has it.
    Repeated,
}

fn valid_instance(text: &str) -> bool {
    text.len() == 16
        && text
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

/// A new instance that is not in `used`, which then holds it.
fn fresh_instance(used: &mut BTreeSet<String>) -> String {
    loop {
        let instance = format!("{:016x}", crate::utils::random_u64());
        if used.insert(instance.clone()) {
            return instance;
        }
    }
}

/// The 1-based line and column (in characters) of a byte offset.
fn position(bytes: &[u8], offset: usize) -> (usize, usize) {
    let before = &bytes[..offset.min(bytes.len())];
    let line_start = before
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |at| at + 1);
    let line = before.iter().filter(|byte| **byte == b'\n').count() + 1;
    let column = String::from_utf8_lossy(&before[line_start..])
        .chars()
        .count()
        + 1;
    (line, column)
}

/// The values a connection's table sets, by config key. Unknown fields are reported, not fatal,
/// so one typo does not take every connection down.
fn entry_values(
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
fn raw_value(item: &Item) -> Option<String> {
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

/// The file's name for a config key: `GITLAB_URL` → `url`, `GIT_DIRS` → `dirs`.
pub fn field_name(source: &Source, key: ProviderConfigKey) -> String {
    let prefix = format!("{}_", source.as_ref().to_ascii_uppercase());
    key.name
        .strip_prefix(&prefix)
        .unwrap_or(key.name)
        .to_ascii_lowercase()
}

/// The config key a file field names for `source`, as the CLI accepts it.
pub fn key_for_field(source: &Source, field: &str) -> Option<ProviderConfigKey> {
    adapter_for(source).ok().and_then(|adapter| {
        adapter
            .config_requirements()
            .iter()
            .copied()
            .find(|key| field_name(source, *key) == field.to_ascii_lowercase())
    })
}

fn adapter_for(source: &Source) -> Result<Arc<dyn ProviderAdapter>> {
    provider_registry()
        .into_iter()
        .find(|adapter| adapter.id() == *source)
        .ok_or_else(|| anyhow!("Unknown provider {source}"))
}

/// A connection's label when none is given: where Git reads, else the kind's name.
fn base_label(source: &Source, location: &ConnectionLocation) -> String {
    match (source, location) {
        (Source::Git, ConnectionLocation::Local) => LOCAL_LABEL.to_string(),
        (Source::Git, ConnectionLocation::Wsl(distro)) => wsl_label(distro),
        _ => source.label().to_string(),
    }
}

/// `wanted`, or `wanted` with the first free number from 2 after `separator`.
fn numbered(wanted: &str, taken: &BTreeSet<String>, separator: &str) -> String {
    if !taken.contains(wanted) {
        return wanted.to_string();
    }
    (2..)
        .map(|n| format!("{wanted}{separator}{n}"))
        .find(|name| !taken.contains(name))
        .expect("an unused name exists")
}

/// The OS folder for settings; `None` without one. There is no fallback: tokens are never
/// written to a temporary or shared folder.
fn config_dir() -> Option<PathBuf> {
    if cfg!(target_os = "windows") {
        env_path("APPDATA")
    } else if cfg!(target_os = "macos") {
        env_path("HOME").map(|home| home.join("Library").join("Application Support"))
    } else {
        env_path("XDG_CONFIG_HOME").or_else(|| env_path("HOME").map(|home| home.join(".config")))
    }
}

/// A path from the environment; an empty variable counts as unset.
fn env_path(name: &str) -> Option<PathBuf> {
    env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}
