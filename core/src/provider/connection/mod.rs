//! A connection is one configured setup of a provider kind: two GitLab accounts are two
//! connections of the `gitlab` kind, each with its own values, status and cache. A kind has
//! connections only once they are added to `config.toml`.

use std::{collections::BTreeMap, sync::Arc};

use crate::{
    model::{ConnectionId, Source},
    provider::provider_config::ConfigSource,
    secrets::sealed::{Malformed, SealedValue},
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

impl ConnectionLocation {
    /// The WSL distribution the connection reads, if it reads one.
    pub fn wsl_distro(&self) -> Option<&str> {
        match self {
            Self::Wsl(distro) => Some(distro),
            Self::Local => None,
        }
    }
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
    /// The table's reserved `instance` id, when it has a readable one: a removed and re-added
    /// connection has another, a legacy table none until it is first written.
    instance: Option<String>,
}

impl std::fmt::Debug for Connection {
    // Values are left out: they hold tokens.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Connection")
            .field("id", &self.id)
            .field("label", &self.label)
            .field("location", &self.location)
            .field("instance", &self.instance)
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
            instance: None,
        }
    }

    pub fn with_instance(mut self, instance: Option<String>) -> Self {
        self.instance = instance;
        self
    }

    pub fn instance(&self) -> Option<&str> {
        self.instance.as_deref()
    }

    pub fn source(&self) -> &Source {
        &self.id.source
    }

    pub fn values(&self) -> &ConnectionValues {
        &self.values
    }

    pub(crate) fn shared_values(&self) -> &Arc<ConnectionValues> {
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

/// A config value as the file holds it: text, or a secret stored sealed.
#[derive(Clone, PartialEq, Eq)]
pub enum StoredValue {
    Text(String),
    Sealed(StoredSecret),
}

/// A secret the file holds as `{ sealed = "…" }`: one whose text reads, or that text and why it
/// does not. Compared by its text, so preconditions and collection keys see a change of it.
#[derive(Clone, PartialEq, Eq)]
pub enum StoredSecret {
    Sealed(SealedValue),
    Unreadable { text: String, malformed: Malformed },
}

impl std::fmt::Debug for StoredValue {
    /// Neither text nor a sealed value's payload: either may be a secret.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Text(_) => f.write_str("Text(<hidden>)"),
            Self::Sealed(StoredSecret::Sealed(sealed)) => write!(f, "Sealed({sealed:?})"),
            Self::Sealed(StoredSecret::Unreadable { malformed, .. }) => {
                write!(f, "Unreadable({malformed:?})")
            }
        }
    }
}

/// A connection's config values by key (`GITLAB_TOKEN`); `None` is a value cleared on purpose.
#[derive(Default, PartialEq, Eq)]
pub struct ConnectionValues(BTreeMap<String, Option<StoredValue>>);

impl ConnectionValues {
    /// Values that are all text.
    pub fn new(values: BTreeMap<String, Option<String>>) -> Self {
        Self(
            values
                .into_iter()
                .map(|(key, value)| (key, value.map(StoredValue::Text)))
                .collect(),
        )
    }

    /// Values as the file holds them.
    pub fn stored(values: BTreeMap<String, Option<StoredValue>>) -> Self {
        Self(values)
    }

    /// The value of `key` as the file holds it; `None` unset or cleared.
    pub fn get_stored(&self, key: &str) -> Option<&StoredValue> {
        self.0.get(key).and_then(Option::as_ref)
    }

    /// The secrets stored sealed, by key.
    pub fn sealed(&self) -> impl Iterator<Item = (&str, &StoredSecret)> {
        self.0.iter().filter_map(|(key, value)| match value {
            Some(StoredValue::Sealed(secret)) => Some((key.as_str(), secret)),
            Some(StoredValue::Text(_)) | None => None,
        })
    }
}

impl ConfigSource for ConnectionValues {
    /// Text only: a sealed secret's text is never handed on as its value.
    fn get(&self, key: &str) -> Option<String> {
        match self.get_stored(key)? {
            StoredValue::Text(text) => Some(text.clone()),
            StoredValue::Sealed(_) => None,
        }
    }
}
