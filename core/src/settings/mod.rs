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

use std::{fs, io, path::Path, sync::Once};

use crate::{platform::private_file::readable_by_others, provider::connection::Connection};

mod config_edit_error;
mod config_error;
mod config_file;
mod content_id;
mod external;
mod keys;
mod precondition;
pub mod view;
mod write;

pub use config_edit_error::{ConfigEditError, LoadError};
pub use config_error::ConfigError;
pub use config_file::{ConfigFile, ResolvedConnections};
pub use content_id::ContentId;
pub use external::{Editor, ExternalEdit, ExternalEditError, edit_externally};
pub use keys::{config_keys, field_name, key_for_field};
pub use precondition::{
    Conflict, ConflictKind, EditBase, FieldId, Precondition, check as check_precondition,
};
pub(crate) use write::lock_wait;
pub use write::{EditError, Edited, edit_config};

const NAME: &str = "name";
const LABEL: &str = "label";
const WSL: &str = "wsl";
/// A reserved, non-secret id that tells two lifetimes of a connection with the same name apart.
/// Written into a table when the app first writes that table.
const INSTANCE: &str = "instance";
/// The one key of a sealed secret's table: `token = { sealed = "v1:…" }`.
const SEALED: &str = "sealed";

/// What every reader and writer says when the OS gives no folder for the settings.
pub const NO_LOCATION: &str = "Activity Log has no folder for its settings. Set ACTIVITY_LOG_CONFIG to a file path, or start it with --config <path>.";

/// Logs [`NO_LOCATION`] once per process.
pub fn warn_no_location() {
    static WARNED: Once = Once::new();
    WARNED.call_once(|| crate::diagnostics::warn("config", NO_LOCATION));
}

/// The connections from the config file at `explicit` (or the default path). Problems in single
/// connections are logged, not returned. Without a settings location there are none.
pub fn load_connections(explicit: Option<&Path>) -> Result<Vec<Connection>, LoadError> {
    match ConfigFile::resolve_path(explicit) {
        Some(path) => Ok(ConfigFile::load(path)?.logged_connections()),
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
