//! A connection is one configured setup of a provider kind: two GitLab accounts are two
//! connections of the `gitlab` kind, each with its own values, status and cache. A kind has
//! connections only once they are added to `config.toml`.

use std::{collections::BTreeMap, sync::Arc};

use crate::{
    model::{ConnectionId, Source},
    provider::provider_config::ConfigSource,
};

#[cfg(test)]
mod tests;

/// Where a connection's data lives. Only Git has locations other than this machine.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ConnectionLocation {
    #[default]
    Local,
    /// Inside a WSL distribution, reached through `wsl.exe`.
    Wsl(String),
}

/// How many connections a kind can have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Instances {
    Single,
    Multiple,
}

#[derive(Clone)]
pub struct Connection {
    pub id: ConnectionId,
    pub label: String,
    pub location: ConnectionLocation,
    values: Arc<ConnectionValues>,
}

impl std::fmt::Debug for Connection {
    // Values are left out: they hold tokens.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Connection")
            .field("id", &self.id)
            .field("label", &self.label)
            .field("location", &self.location)
            .finish()
    }
}

impl Connection {
    pub fn new(
        id: ConnectionId,
        label: impl Into<String>,
        location: ConnectionLocation,
        values: ConnectionValues,
    ) -> Self {
        Self {
            id,
            label: label.into(),
            location,
            values: Arc::new(values),
        }
    }

    pub fn source(&self) -> &Source {
        &self.id.source
    }

    pub fn values(&self) -> &ConnectionValues {
        &self.values
    }
}

/// A name for ids and file names: lowercase ASCII letters, digits and dashes.
pub fn slug(label: &str) -> String {
    let mut slug = String::new();
    for c in label.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    let slug = slug.trim_end_matches('-');
    if slug.is_empty() {
        "connection".to_string()
    } else {
        slug.to_string()
    }
}

/// A connection's config values by key (`GITLAB_TOKEN`); `None` is a value cleared on purpose.
#[derive(Default)]
pub struct ConnectionValues(BTreeMap<String, Option<String>>);

impl ConnectionValues {
    pub fn new(values: BTreeMap<String, Option<String>>) -> Self {
        Self(values)
    }
}

impl ConfigSource for ConnectionValues {
    fn get(&self, key: &str) -> Option<String> {
        self.0.get(key).cloned().flatten()
    }
}
