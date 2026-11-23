//! `legacy-import.toml`, beside `config.toml`: the record that 0.0.7's settings were imported,
//! written under the config lock once `config.toml` is published, and what it imported and
//! left out, never a value. Its presence alone stops another import; one that does not read
//! still counts, and says so.

use std::path::Path;

use toml_edit::{Array, ArrayOfTables, DocumentMut, Table, value};

use crate::model::{LegacyImportSkipDto, LegacySource};

#[cfg(test)]
mod tests;

pub(super) const MARKER: &str = "legacy-import.toml";
/// The one release this import reads.
const RELEASE: &str = "0.0.7";

/// What an import did, as the marker records it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Record {
    /// RFC 3339.
    pub(super) at: String,
    pub(super) used: LegacySource,
    pub(super) path: String,
    pub(super) found_unused: Vec<String>,
    pub(super) imported: Vec<String>,
    pub(super) skipped: Vec<LegacyImportSkipDto>,
}

impl Record {
    pub(super) fn to_toml(&self) -> String {
        let mut doc = DocumentMut::new();
        doc["release"] = value(RELEASE);
        doc["at"] = value(&self.at);
        doc["used"] = value(wire_name(&self.used));
        doc["path"] = value(&self.path);
        doc["found_unused"] = value(self.found_unused.iter().collect::<Array>());
        doc["imported"] = value(self.imported.iter().collect::<Array>());
        let mut skipped = ArrayOfTables::new();
        for skip in &self.skipped {
            let mut table = Table::new();
            if let Some(connection) = &skip.connection {
                table["connection"] = value(connection.to_string());
            }
            table["field"] = value(&skip.field);
            table["reason"] = value(wire_name(&skip.reason));
            skipped.push(table);
        }
        if !skipped.is_empty() {
            doc["skipped"] = toml_edit::Item::ArrayOfTables(skipped);
        }
        doc.to_string()
    }
}

/// The marker of an import given up on purpose (`activity-log config skip-import`): nothing
/// was imported, and nothing will be.
pub(super) fn given_up_toml(at: &str) -> String {
    let mut doc = DocumentMut::new();
    doc["release"] = value(RELEASE);
    doc["at"] = value(at);
    doc["given_up"] = value(true);
    doc.to_string()
}

/// A unit variant's name as the contract spells it.
fn wire_name(variant: &impl serde::Serialize) -> String {
    match serde_json::to_value(variant) {
        Ok(serde_json::Value::String(name)) => name,
        _ => unreachable!("a unit variant serializes as its name"),
    }
}

/// What lies at the marker's place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Marker {
    Absent,
    Present,
    /// Something is there that is no marker of this import: it counts as present.
    Unreadable,
}

/// The marker beside `config`, by its presence; its content is only checked to be one.
pub(super) fn marker_beside(config: &Path) -> Marker {
    let path = config.with_file_name(MARKER);
    if std::fs::symlink_metadata(&path).is_err() {
        return Marker::Absent;
    }
    let read = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| text.parse::<DocumentMut>().ok());
    match read {
        Some(doc) if doc.get("release").and_then(|item| item.as_str()) == Some(RELEASE) => {
            Marker::Present
        }
        _ => Marker::Unreadable,
    }
}
