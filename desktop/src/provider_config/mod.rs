//! Where the desktop reads `config.toml` for the settings pane, and the files its snapshots
//! were built from, so a save or removal based on one is checked against what it showed. The
//! view itself, its checks and its edits are `core::settings::view`.

use std::{
    collections::VecDeque,
    path::Path,
    sync::{Arc, Mutex, PoisonError},
};

use activity_log_core::{
    model::{BackendError, ConnectionId},
    provider::connection::Connection,
    settings::{ConfigFile, FieldId, view::SaveBase, warn_no_location},
    utils::random_u64,
};

#[cfg(test)]
mod tests;

/// How many snapshots a save can still be based on; an older one is `UnknownBase`.
const HELD: usize = 16;

/// The settings file at `path`, `None` without a settings location.
pub(crate) fn load_config(path: Option<&Path>) -> Result<Option<ConfigFile>, BackendError> {
    match path {
        Some(path) => Ok(Some(ConfigFile::load(path)?)),
        None => {
            warn_no_location();
            Ok(None)
        }
    }
}

/// The connections the settings pane shows. Its problems were logged when the file was read.
pub(crate) fn shown_connections(file: &ConfigFile) -> Vec<Connection> {
    file.connections().connections
}

/// The files of the latest snapshots handed out, by revision. In process memory only: a file
/// holds tokens.
#[derive(Default)]
pub(crate) struct SnapshotRegistry {
    held: Mutex<VecDeque<(String, Arc<ConfigFile>)>>,
}

impl SnapshotRegistry {
    /// Holds `file` and names it; a snapshot built from it carries the name as its revision.
    pub(crate) fn register(&self, file: ConfigFile) -> String {
        let revision = format!("{:016x}", random_u64());
        let mut held = self.held.lock().unwrap_or_else(PoisonError::into_inner);
        held.push_back((revision.clone(), Arc::new(file)));
        while held.len() > HELD {
            held.pop_front();
        }
        revision
    }

    /// What a request for `id` based on `based_on` saw of the `touched` fields; a revision no
    /// longer held (or one without `id`) is an unknown base.
    pub(crate) fn base(&self, based_on: &str, id: &ConnectionId, touched: &[FieldId]) -> SaveBase {
        let file = self
            .held
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .find(|(held, _)| held == based_on)
            .map(|(_, file)| Arc::clone(file));
        match file.and_then(|file| file.edit_base(id, touched)) {
            Some(base) => SaveBase::Known(base),
            None => SaveBase::Unknown(id.clone()),
        }
    }
}
