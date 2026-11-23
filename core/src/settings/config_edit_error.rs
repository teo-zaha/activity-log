use std::path::PathBuf;

use crate::model::{ConnectionId, Source};

use super::ConfigError;

/// Why a change to `config.toml` was refused; nothing was written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigEditError {
    /// No connection has this id (another program may have removed it).
    UnknownConnection(ConnectionId),
    /// The kind allows one connection, and it has one.
    SingleInstance(Source),
    WslOnlyForGit,
    EmptyLabel,
    /// The kind has no such setting.
    UnknownSetting {
        source: Source,
        key: &'static str,
    },
    /// The kind's entry in the file is not a list of tables (`gitlab = "x"`).
    NotATableList(Source),
    /// What a connection's sealed secret is bound to (its server's address, Jira's TLS
    /// setting) changed without the secret: it would not open, so it must be entered again.
    BindingChanged {
        connection: ConnectionId,
        /// The secret's wire name, `GITLAB_TOKEN`.
        secret: &'static str,
    },
}

impl std::fmt::Display for ConfigEditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownConnection(id) => write!(f, "No connection {id}"),
            Self::SingleInstance(source) => write!(f, "{} is already added", source.label()),
            Self::WslOnlyForGit => write!(f, "Only Git connections can be in WSL"),
            Self::EmptyLabel => write!(f, "A connection needs a label"),
            Self::UnknownSetting { source, key } => {
                write!(f, "{} has no setting {key}", source.label())
            }
            Self::NotATableList(source) => {
                write!(f, "`{}` must be a list of tables", source.as_ref())
            }
            Self::BindingChanged { connection, secret } => write!(
                f,
                "{connection}: enter {secret} again for the new address or TLS setting"
            ),
        }
    }
}

impl std::error::Error for ConfigEditError {}

/// Why the config file could not be loaded.
#[derive(Debug)]
pub enum LoadError {
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    Invalid(ConfigError),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Read { path, source } => write!(f, "Reading {}: {source}", path.display()),
            Self::Invalid(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for LoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Read { source, .. } => Some(source),
            Self::Invalid(err) => Some(err),
        }
    }
}
