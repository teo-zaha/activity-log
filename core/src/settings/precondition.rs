//! What a writer saw of a connection, and the check that it is still so under the config lock.

use std::collections::BTreeMap;

use super::ConfigFile;
use crate::{model::ConnectionId, provider::connection::StoredValue};

/// A field a writer can change: the label or a config value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum FieldId {
    Label,
    /// A config key's wire name, `GITLAB_TOKEN`.
    Value(&'static str),
}

/// What a writer saw of one connection. Held in this process only: it includes secrets.
#[derive(Clone)]
pub struct EditBase {
    connection: ConnectionId,
    /// The connection's label as the writer saw it, which a conflict names it by.
    label: String,
    /// `None` for a legacy table that had no instance when the writer read it.
    instance: Option<String>,
    /// The touched fields and the kind's account fields as the file held them (a sealed
    /// secret by its text); `None` for unset.
    fields: BTreeMap<FieldId, Option<StoredValue>>,
}

impl EditBase {
    pub(super) fn new(
        connection: ConnectionId,
        label: String,
        instance: Option<String>,
        fields: BTreeMap<FieldId, Option<StoredValue>>,
    ) -> Self {
        Self {
            connection,
            label,
            instance,
            fields,
        }
    }

    pub fn connection(&self) -> &ConnectionId {
        &self.connection
    }
}

impl std::fmt::Debug for EditBase {
    /// Without the values, which may be secrets.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EditBase")
            .field("connection", &self.connection)
            .field("instance", &self.instance)
            .field("fields", &self.fields.keys().collect::<Vec<_>>())
            .finish()
    }
}

#[derive(Debug, Clone)]
pub enum Precondition {
    /// The writer showed nothing it could be based on: a command with every value given.
    None,
    Unchanged(EditBase),
    /// The writer creates the file: one that exists under the lock, even empty, is another
    /// writer's, and nothing is written ([`EditError::FileExists`](super::EditError)).
    FileAbsent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictKind {
    Removed,
    /// Removed and added again, or rewritten by a writer outside the protocol.
    Replaced,
    Changed,
    /// The writer's base is no longer known (an evicted settings snapshot).
    UnknownBase,
}

/// What failed, without any value. `fields` is empty when the connection itself is gone or
/// replaced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub connection: ConnectionId,
    /// The connection's label as the writer saw it; `None` when its base is unknown.
    pub label: Option<String>,
    pub kind: ConflictKind,
    pub fields: Vec<FieldId>,
}

impl Conflict {
    /// The message a writer shows, naming the connection by `label`.
    pub fn message(&self, label: &str) -> String {
        match self.kind {
            ConflictKind::Changed => format!(
                "{label} was changed by another program while you were editing it. Nothing was saved."
            ),
            ConflictKind::Removed | ConflictKind::Replaced | ConflictKind::UnknownBase => {
                format!("{label} was removed or replaced by another program. Nothing was saved.")
            }
        }
    }
}

/// Checks `precondition` against the file as read under the lock, or, for a check before a
/// save (`view::check_save`), against the file as it is now.
pub fn check(file: &ConfigFile, precondition: &Precondition) -> Result<(), Conflict> {
    let Precondition::Unchanged(base) = precondition else {
        return Ok(());
    };
    let conflict = |kind, fields| Conflict {
        connection: base.connection.clone(),
        label: Some(base.label.clone()),
        kind,
        fields,
    };
    let fields = base.fields.keys().cloned().collect::<Vec<_>>();
    let Some(current) = file.seen(&base.connection, &fields) else {
        return Err(conflict(ConflictKind::Removed, Vec::new()));
    };
    // A table that had no instance and still has none is compared by its values; once another
    // writer gave it one, the two cannot be told apart from a re-added connection.
    if current.instance != base.instance {
        return Err(conflict(ConflictKind::Replaced, Vec::new()));
    }
    let changed = base
        .fields
        .iter()
        .filter(|(field, seen)| current.fields.get(*field) != Some(*seen))
        .map(|(field, _)| field.clone())
        .collect::<Vec<_>>();
    if changed.is_empty() {
        Ok(())
    } else {
        Err(conflict(ConflictKind::Changed, changed))
    }
}
