//! `activity-log config edit`: the user's editor works on a private copy, never on the file
//! itself. Saving publishes the edited text through the same protocol as every other writer;
//! closing the editor without a change leaves the file exactly as it was, or missing.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    io::{Read, Write},
    ops::Range,
    path::{Path, PathBuf},
};

use anyhow::anyhow;
use toml_edit::{Item, Table, Value};

use super::{
    ConfigFile, INSTANCE, fresh_instance, provider_registry, set_instance, valid_instance,
    warn_if_shared,
    write::{self, EditError},
};

mod editor;
#[cfg(test)]
mod tests;

pub use editor::Editor;

#[derive(Debug)]
pub enum ExternalEdit {
    /// The copy came back as it went out; nothing was written.
    Unchanged,
    /// The edited text was published; problems in single connections are for the caller to
    /// log.
    Saved { warnings: Vec<String> },
}

#[derive(Debug)]
pub enum ExternalEditError {
    /// Nothing was saved and nothing is left behind.
    Failed(EditError),
    /// Nothing was saved; the user's version stays at `copy`.
    Kept { copy: PathBuf, cause: EditError },
}

impl std::fmt::Display for ExternalEditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Failed(err) => write!(f, "{err}"),
            Self::Kept { copy, cause } => write!(
                f,
                "{}; your version is in {}",
                cause.to_string().trim_end_matches('.'),
                copy.display()
            ),
        }
    }
}

impl std::error::Error for ExternalEditError {
    /// The typed cause, which the message above already says.
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Failed(cause) | Self::Kept { cause, .. } => Some(cause),
        }
    }
}

/// Copies the config file at `path` into a private session folder beside it, runs `editor` on
/// the copy without holding the lock, and publishes the copy if it changed and the file did
/// not change meanwhile. The copy, the comparison and the publication all go through one
/// handle to the file's folder.
pub fn edit_externally(
    path: &Path,
    editor: impl FnOnce(&Path) -> anyhow::Result<()>,
) -> Result<ExternalEdit, ExternalEditError> {
    let (folder, path) = write::open_parent(path).map_err(ExternalEditError::Failed)?;
    let name = write::file_name(&path).map_err(ExternalEditError::Failed)?;
    let failed = |err: std::io::Error| ExternalEditError::Failed(EditError::Internal(anyhow!(err)));
    let session = folder
        .staging(&name)
        .map_err(write::unprotectable_or(|err| {
            EditError::Internal(anyhow!(err))
        }))
        .map_err(ExternalEditError::Failed)?;
    let original = folder.read(&name).map_err(failed)?;
    if original.is_some() {
        warn_if_shared(&path);
    }
    let mut copy = session.create(&name).map_err(failed)?;
    copy.write_all(original.as_deref().unwrap_or_default())
        .map_err(failed)?;
    drop(copy);
    // The editor writes or replaces it; it is read back and checked afterwards.
    session.release(&name);
    let copy_path = session.path().join(&name);

    let ran = editor(&copy_path);
    let mut edited = Vec::new();
    let read = session
        .open(&name)
        .and_then(|mut copy| copy.read_to_end(&mut edited));
    if read.is_ok() && edited == original.as_deref().unwrap_or_default() {
        return match ran {
            Ok(()) => Ok(ExternalEdit::Unchanged),
            Err(err) => Err(ExternalEditError::Failed(EditError::Internal(err))),
        };
    }
    // From here on the user's version is never removed unless it was saved.
    let keep = |cause: EditError| ExternalEditError::Kept {
        copy: session.keep().join(&name),
        cause,
    };
    if let Err(err) = ran {
        return Err(keep(EditError::Internal(err)));
    }
    if let Err(err) = read {
        return Err(keep(EditError::Internal(
            anyhow!(err).context("Reading your version failed"),
        )));
    }

    let saved = (|| {
        let locked = write::lock_in(folder, path)?;
        let current = locked.read()?;
        if current != original {
            return Err(EditError::FileChanged { name: name.clone() });
        }
        let mut file = ConfigFile::parse(&copy_path, &edited).map_err(EditError::Invalid)?;
        // A file that did not parse is being repaired: every table of the copy is new.
        let original = original
            .as_deref()
            .and_then(|bytes| ConfigFile::parse(&locked.path, bytes).ok());
        let fresh = fresh_instances(original.as_ref(), &file);
        let content = if fresh.is_empty() {
            edited
        } else {
            // Each planned instance must read back for its own table, however the text was
            // changed; the user's text is kept where it can be.
            let spliced = splice(&edited, &fresh).filter(|content| reads_back(content, &fresh));
            let content = spliced.or_else(|| {
                for (kind, index, instance) in &fresh {
                    let table = file
                        .doc
                        .get_mut(kind)
                        .and_then(Item::as_array_of_tables_mut)
                        .and_then(|tables| tables.get_mut(*index))?;
                    set_instance(table, instance);
                }
                let content = write::with_line_endings_of(file.to_string(), Some(&edited));
                reads_back(&content, &fresh).then_some(content)
            });
            content.ok_or_else(|| {
                EditError::Internal(anyhow!(
                    "Activity Log could not give the connections their `instance` ids"
                ))
            })?
        };
        let written = locked.write_checked(current.as_deref(), content)?;
        Ok(written.warnings)
    })();
    match saved {
        Ok(warnings) => Ok(ExternalEdit::Saved { warnings }),
        Err(err) => Err(keep(err)),
    }
}

/// The tables of the edited document that need a fresh instance, as `(kind, index, instance)`,
/// compared with the original table of the same kind and name:
/// - a new table gets one, replacing any it carries;
/// - a table that had one keeps it only if it still has it and no other table does;
/// - a table that had none stays as written only while it is unchanged.
fn fresh_instances(
    original: Option<&ConfigFile>,
    edited: &ConfigFile,
) -> Vec<(String, usize, String)> {
    let mut counts = HashMap::<String, usize>::new();
    for (_, _, table) in edited.tables_in_file_order() {
        if let Some(instance) = written_instance(table) {
            *counts.entry(instance.to_string()).or_default() += 1;
        }
    }
    let mut used = counts.keys().cloned().collect::<BTreeSet<_>>();
    let instances = edited.instances();
    let mut fresh = Vec::new();
    for adapter in provider_registry() {
        let source = adapter.id();
        let before = original
            .map(|original| original.entries(&source))
            .unwrap_or_default()
            .into_iter()
            .map(|entry| (entry.id.name.clone(), entry))
            .collect::<BTreeMap<_, _>>();
        for entry in edited.entries_with_warnings(&source, &instances, &mut Vec::new()) {
            let keeps = match before.get(&entry.id.name) {
                None => false,
                Some(old) => match &old.instance {
                    Some(instance) => {
                        written_instance(&entry.table) == Some(instance.as_str())
                            && counts.get(instance) == Some(&1)
                    }
                    None => canonical_table(&old.table) == canonical_table(&entry.table),
                },
            };
            if !keeps {
                let instance = fresh_instance(&mut used);
                fresh.push((source.as_ref().to_string(), entry.index, instance));
            }
        }
    }
    fresh
}

/// `text` with the given tables' `instance` set, changing nothing else: a value is replaced in
/// place, a missing line goes right after the table's header with the file's line ending.
/// `None` when the text cannot be mapped.
fn splice(text: &[u8], fresh: &[(String, usize, String)]) -> Option<Vec<u8>> {
    let text = std::str::from_utf8(text).ok()?;
    let doc = toml_edit::Document::parse(text).ok()?;
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut edits: Vec<(Range<usize>, String)> = Vec::new();
    for (kind, index, instance) in fresh {
        let table = doc.get(kind)?.as_array_of_tables()?.get(*index)?;
        let quoted = format!("\"{instance}\"");
        match table.get(INSTANCE) {
            // Only a plain `instance = …` value is replaced in place. A table (`instance.x`,
            // `[kind.instance]`) is no value: its span is a key or header.
            Some(Item::Value(value)) => edits.push((value.span()?, quoted)),
            Some(_) => return None,
            None => {
                let header = table.span()?;
                let line_end = text[header.end..]
                    .find('\n')
                    .map_or(text.len(), |at| header.end + at + 1);
                let lead = if line_end == text.len() && !text.ends_with('\n') {
                    newline
                } else {
                    ""
                };
                edits.push((
                    line_end..line_end,
                    format!("{lead}{INSTANCE} = {quoted}{newline}"),
                ));
            }
        }
    }
    edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let mut spliced = text.to_string();
    for (range, replacement) in edits {
        spliced.replace_range(range, &replacement);
    }
    Some(spliced.into_bytes())
}

/// Whether `content` reads each planned instance as its table's own.
fn reads_back(content: &[u8], fresh: &[(String, usize, String)]) -> bool {
    let Ok(file) = ConfigFile::parse("config.toml", content) else {
        return false;
    };
    let instances = file.instances();
    fresh.iter().all(|(kind, index, instance)| {
        instances.get(&(kind.clone(), *index))
            == Some(&super::InstanceRead::Valid(instance.clone()))
    })
}

/// The table's `instance` if it is well formed, before the rule for repeated ones.
fn written_instance(table: &Table) -> Option<&str> {
    table
        .get(INSTANCE)
        .and_then(Item::as_str)
        .filter(|text| valid_instance(text))
}

/// A table's keys and typed values, without formatting or comments: `1` and `'1'` differ,
/// `[1,2]` and `[ 1, 2 ]` do not.
fn canonical_table(table: &Table) -> String {
    let fields = table
        .iter()
        .map(|(key, item)| format!("{key:?}={}", canonical_item(item)))
        .collect::<BTreeSet<_>>();
    format!("{{{}}}", fields.into_iter().collect::<Vec<_>>().join(","))
}

fn canonical_item(item: &Item) -> String {
    match item {
        Item::None => String::new(),
        Item::Value(value) => canonical_value(value),
        Item::Table(table) => canonical_table(table),
        Item::ArrayOfTables(tables) => format!(
            "[{}]",
            tables
                .iter()
                .map(canonical_table)
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
}

fn canonical_value(value: &Value) -> String {
    match value {
        Value::String(text) => format!("s{:?}", text.value()),
        Value::Integer(number) => format!("i{}", number.value()),
        Value::Float(number) => format!("f{:?}", number.value()),
        Value::Boolean(flag) => format!("b{}", flag.value()),
        Value::Datetime(datetime) => format!("d{}", datetime.value()),
        Value::Array(array) => format!(
            "[{}]",
            array
                .iter()
                .map(canonical_value)
                .collect::<Vec<_>>()
                .join(",")
        ),
        Value::InlineTable(table) => {
            let fields = table
                .iter()
                .map(|(key, value)| format!("{key:?}={}", canonical_value(value)))
                .collect::<BTreeSet<_>>();
            format!("{{{}}}", fields.into_iter().collect::<Vec<_>>().join(","))
        }
    }
}
