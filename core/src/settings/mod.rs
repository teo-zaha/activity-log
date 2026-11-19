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
//! The file is the only list of sources: a kind it does not mention has none.

use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, Result, anyhow, bail};
use toml_edit::{ArrayOfTables, DocumentMut, Item, Table, Value, value};

use crate::{
    model::{ConnectionId, Source},
    provider::{
        connection::{Connection, ConnectionLocation, ConnectionValues, Instances, slug},
        git::{LOCAL_LABEL, wsl_label},
        provider_adapter::ProviderAdapter,
        provider_config::{
            ProviderConfigKey, ProviderConfigShape, ProviderConfigValueType, encode_config_list,
            parse_config_list,
        },
        provider_registry,
    },
};

#[cfg(test)]
mod tests;

const NAME: &str = "name";
const LABEL: &str = "label";
const WSL: &str = "wsl";

/// The connections from the config file at `explicit` (or the default path). Problems in single
/// connections are logged, not returned.
pub fn load_connections(explicit: Option<&Path>) -> Result<Vec<Connection>> {
    ConfigFile::load(ConfigFile::resolve_path(explicit))?.logged_connections()
}

/// The connections file and its path.
#[derive(Debug, Clone)]
pub struct ConfigFile {
    path: PathBuf,
    doc: DocumentMut,
}

/// Connections read from the file, with problems that did not stop reading it.
#[derive(Debug, Clone)]
pub struct ResolvedConnections {
    pub connections: Vec<Connection>,
    pub warnings: Vec<String>,
}

impl ConfigFile {
    /// `explicit`, else `ACTIVITY_LOG_CONFIG`, else `activity-log/config.toml` in the OS config
    /// directory.
    pub fn resolve_path(explicit: Option<&Path>) -> PathBuf {
        if let Some(path) = explicit {
            return path.to_path_buf();
        }
        if let Some(path) = env::var_os("ACTIVITY_LOG_CONFIG") {
            return PathBuf::from(path);
        }
        config_dir().join("activity-log").join("config.toml")
    }

    /// Reads the file; a missing file is an empty configuration.
    pub fn load(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(err) => return Err(err).with_context(|| format!("Reading {}", path.display())),
        };
        Self::parse(path, &text)
    }

    pub fn parse(path: impl Into<PathBuf>, text: &str) -> Result<Self> {
        let path = path.into();
        let doc = text
            .parse::<DocumentMut>()
            .with_context(|| format!("{} is not valid TOML", path.display()))?;
        Ok(Self { path, doc })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Writes the file, readable only by the user on Unix since it holds tokens.
    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("Creating {}", parent.display()))?;
        }
        fs::write(&self.path, self.doc.to_string())
            .with_context(|| format!("Writing {}", self.path.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&self.path, fs::Permissions::from_mode(0o600))
                .with_context(|| format!("Restricting {}", self.path.display()))?;
        }
        Ok(())
    }

    /// Every connection, with the problems in single connections logged instead of returned.
    pub fn logged_connections(&self) -> Result<Vec<Connection>> {
        let resolved = self.connections()?;
        for warning in resolved.warnings {
            crate::diagnostics::warn("config", warning);
        }
        Ok(resolved.connections)
    }

    /// Every connection of every provider kind, in the file's order.
    pub fn connections(&self) -> Result<ResolvedConnections> {
        let mut resolved = ResolvedConnections {
            connections: Vec::new(),
            warnings: Vec::new(),
        };
        for adapter in provider_registry() {
            for entry in self.entries_with_warnings(&adapter.id(), &mut resolved.warnings)? {
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
        let existing = self.entries(source)?;
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
        self.pin_names(source)?;
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
        Ok(ConnectionId::new(source.clone(), name))
    }

    pub fn remove(&mut self, id: &ConnectionId) -> Result<()> {
        self.pin_names(&id.source)?;
        let index = self.index_of(id)?;
        let tables = self.tables_mut(&id.source)?;
        tables.remove(index);
        if tables.is_empty() {
            self.doc.remove(id.source.as_ref());
        }
        Ok(())
    }

    pub fn rename(&mut self, id: &ConnectionId, label: &str) -> Result<()> {
        let label = label.trim();
        if label.is_empty() {
            bail!("A connection needs a label");
        }
        self.pin_names(&id.source)?;
        let index = self.index_of(id)?;
        self.table_mut(&id.source, index)?[LABEL] = value(label);
        Ok(())
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
        self.pin_names(&id.source)?;
        let index = self.index_of(id)?;
        let field = field_name(&id.source, key);
        let table = self.table_mut(&id.source, index)?;
        match raw {
            Some(raw) => {
                table[field.as_str()] = typed_item(key, raw);
            }
            None => {
                table.remove(&field);
            }
        }
        Ok(())
    }

    /// The connections of a kind as written in the file.
    fn entries(&self, source: &Source) -> Result<Vec<Entry>> {
        self.entries_with_warnings(source, &mut Vec::new())
    }

    /// The connections of a kind with unique ids. Names are compared as slugs, explicit names
    /// first so a table without one never takes them; a repeated name is numbered (`work-2`) in
    /// file order and reported, rather than failing every connection. The next edit writes the
    /// numbered name into the file.
    fn entries_with_warnings(
        &self,
        source: &Source,
        warnings: &mut Vec<String>,
    ) -> Result<Vec<Entry>> {
        let Some(item) = self.doc.get(source.as_ref()) else {
            return Ok(Vec::new());
        };
        let tables: Vec<&Table> = match item {
            Item::ArrayOfTables(tables) => tables.iter().collect(),
            Item::Value(Value::Array(array)) if array.is_empty() => Vec::new(),
            _ => bail!(
                "`{}` must be a list of [[{}]] tables",
                source.as_ref(),
                source.as_ref()
            ),
        };

        let explicit = tables
            .iter()
            .map(|table| table.get(NAME).and_then(Item::as_str).map(slug))
            .collect::<Vec<_>>();
        let mut taken = BTreeSet::new();
        let first_claims = explicit
            .iter()
            .map(|name| name.as_ref().is_some_and(|name| taken.insert(name.clone())))
            .collect::<Vec<_>>();

        let mut entries = Vec::new();
        for ((table, explicit), first_claim) in tables.into_iter().zip(explicit).zip(first_claims) {
            let location = match table.get(WSL).and_then(Item::as_str) {
                Some(distro) if *source == Source::Git => {
                    ConnectionLocation::Wsl(distro.to_string())
                }
                Some(_) => bail!("Only [[git]] connections can set `wsl`"),
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
            entries.push(Entry {
                id: ConnectionId::new(source.clone(), name),
                label,
                location,
                table: table.clone(),
            });
        }
        Ok(entries)
    }

    /// Writes every table's name and label of a kind before it is edited, so ids derived from
    /// labels stay as they were.
    fn pin_names(&mut self, source: &Source) -> Result<()> {
        let entries = self.entries(source)?;
        if entries.is_empty() {
            return Ok(());
        }
        let tables = self.tables_mut(source)?;
        for (table, entry) in tables.iter_mut().zip(entries) {
            // Indexing keeps a key's comments, unlike `insert`.
            if table.get(NAME).and_then(Item::as_str) != Some(entry.id.name.as_str()) {
                table[NAME] = value(entry.id.name.as_str());
            }
            if table.get(LABEL).and_then(Item::as_str) != Some(entry.label.as_str()) {
                table[LABEL] = value(entry.label.as_str());
            }
        }
        Ok(())
    }

    fn index_of(&self, id: &ConnectionId) -> Result<usize> {
        self.entries(&id.source)?
            .iter()
            .position(|entry| entry.id == *id)
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
    label: String,
    location: ConnectionLocation,
    table: Table,
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
        if matches!(field, NAME | LABEL | WSL) {
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

fn config_dir() -> PathBuf {
    let home = || env::var_os("HOME").map(PathBuf::from);
    let base = if cfg!(target_os = "windows") {
        env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        home().map(|home| home.join("Library").join("Application Support"))
    } else {
        env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| home().map(|home| home.join(".config")))
    };
    base.unwrap_or_else(env::temp_dir)
}
