use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use toml_edit::DocumentMut;

use self::edit::Refresh;
use super::{ConfigError, LoadError, config_error::position, read_config_bytes};
use crate::{model::ConnectionId, provider::connection::Connection};

mod edit;
pub(super) mod instances;
mod read;
mod sealing;

#[cfg(test)]
mod tests;

/// The connections file and its path.
#[derive(Clone)]
pub struct ConfigFile {
    path: PathBuf,
    pub(super) doc: DocumentMut,
    /// Connections added or changed since the file was read, and whether they need a new
    /// `instance` even if they have one; they get it when the file is written.
    touched: BTreeMap<ConnectionId, Refresh>,
}

impl std::fmt::Debug for ConfigFile {
    // The document is left out: it holds tokens.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConfigFile")
            .field("path", &self.path)
            .field("touched", &self.touched)
            .finish_non_exhaustive()
    }
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
    /// directory: the settings then have no location (see [`NO_LOCATION`](super::NO_LOCATION)).
    pub fn resolve_path(explicit: Option<&Path>) -> Option<PathBuf> {
        let path = explicit
            .map(Path::to_path_buf)
            .or_else(|| crate::platform::dirs::env_path("ACTIVITY_LOG_CONFIG"))
            // No fallback without an OS folder: tokens are never written to a temporary or
            // shared folder.
            .or_else(|| {
                crate::platform::dirs::config_home().map(|dir| {
                    dir.join(crate::profile::current().folder())
                        .join("config.toml")
                })
            })?;
        Some(std::path::absolute(&path).unwrap_or(path))
    }

    /// Reads the file; a missing file is an empty configuration.
    pub fn load(path: impl Into<PathBuf>) -> Result<Self, LoadError> {
        let path = path.into();
        let bytes = match read_config_bytes(&path) {
            Ok(bytes) => bytes.unwrap_or_default(),
            Err(source) => return Err(LoadError::Read { path, source }),
        };
        Self::parse(path, &bytes).map_err(LoadError::Invalid)
    }

    /// A file without connections, at no location: what the settings show without one.
    pub fn empty() -> Self {
        Self::parse(PathBuf::new(), b"").expect("an empty file parses")
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
}

impl std::fmt::Display for ConfigFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.doc)
    }
}
