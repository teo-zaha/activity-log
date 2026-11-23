//! A connection's sealed secrets, opened once for its check and every later collection: with
//! the key its store holds (asked once, the reader's [`Secrets::key`]), for the binding its
//! table names now. What did not open says why, in words that tell the user what to do.

use std::{collections::BTreeMap, fmt};

use zeroize::Zeroizing;

use super::{
    Secrets, StoreError, StoreErrorKind,
    sealed::{Binding, BindingError, Malformed, Unopenable, open},
};
use crate::{
    model::{ProviderProblem, Source},
    provider::{
        adapter,
        connection::{ConnectionValues, StoredSecret},
        provider_config::{ConfigSource, ProviderConfigKey},
    },
};

#[cfg(test)]
mod tests;

/// The plain text of each sealed secret of a connection, by its wire name.
#[derive(Default)]
pub struct Opened(BTreeMap<&'static str, Zeroizing<String>>);

impl Opened {
    /// `values` as a provider reads them: each opened secret in place of its sealed text.
    pub fn over<'a>(&'a self, values: &'a ConnectionValues) -> impl ConfigSource + 'a {
        WithOpened {
            values,
            opened: self,
        }
    }
}

impl fmt::Debug for Opened {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Opened")
            .field(&self.0.keys().collect::<Vec<_>>())
            .finish()
    }
}

struct WithOpened<'a> {
    values: &'a ConnectionValues,
    opened: &'a Opened,
}

impl ConfigSource for WithOpened<'_> {
    fn get(&self, key: &str) -> Option<String> {
        match self.opened.0.get(key) {
            Some(text) => Some(text.as_str().to_string()),
            None => self.values.get(key),
        }
    }
}

/// Why a connection's sealed secret did not open: the first that did not decides.
#[derive(Debug)]
pub enum Unopened {
    /// The store answered and holds no key: none was ever made on this computer, or it was
    /// deleted (a reset keychain).
    NoKey { field: ProviderConfigKey },
    Unopenable {
        field: ProviderConfigKey,
        why: Unopenable,
    },
    /// The sealed text does not read: written by a newer Activity Log, or damaged.
    Unreadable {
        field: ProviderConfigKey,
        why: Malformed,
    },
    /// The address it would be bound to is no web address it can be sent to.
    Unbound {
        field: ProviderConfigKey,
        why: BindingError,
    },
    /// The store gave no answer: missing, refusing or failing.
    Store {
        field: ProviderConfigKey,
        error: StoreError,
    },
}

impl Unopened {
    /// The connection's problem: the store's trouble is `secretUnavailable` (check again once
    /// it answers); anything else is a value to enter again, `notConfigured`.
    pub fn problem(&self) -> ProviderProblem {
        match self {
            Self::Store { .. } => ProviderProblem::SecretUnavailable,
            Self::NoKey { .. }
            | Self::Unopenable { .. }
            | Self::Unreadable { .. }
            | Self::Unbound { .. } => ProviderProblem::NotConfigured,
        }
    }

    /// What to tell the user; for people only, never decided by.
    pub fn reason(&self) -> String {
        match self {
            Self::NoKey { field }
            | Self::Unopenable {
                field,
                why: Unopenable::OtherKey,
            } => format!(
                "The {} was saved on another computer, or before this computer's keychain was reset. Enter it again.",
                field.label
            ),
            Self::Unopenable {
                field,
                why: Unopenable::OtherAddress,
            } => format!(
                "The {} was saved for another server or TLS setting. Enter it again.",
                field.label
            ),
            Self::Unopenable {
                field,
                why: Unopenable::Damaged | Unopenable::OtherKind,
            } => format!("The stored {} cannot be read. Enter it again.", field.label),
            Self::Unreadable {
                field,
                why: Malformed::Version,
            } => format!(
                "The {} was saved by a newer version of Activity Log.",
                field.label
            ),
            Self::Unreadable { field, why } => {
                format!(
                    "The stored {} cannot be read: {why}. Enter it again.",
                    field.label
                )
            }
            Self::Unbound { field, why } => {
                format!("The {} cannot be used until {why}.", field.label)
            }
            Self::Store { error, .. } => match error.kind() {
                StoreErrorKind::Unavailable => format!("There is no secure store: {error}"),
                StoreErrorKind::Refused => {
                    format!("The secure store did not give the key: {error}")
                }
                StoreErrorKind::Failed => format!("The secure store failed: {error}"),
            },
        }
    }
}

/// Opens each sealed secret of a connection of `source`, in its kind's order of keys.
pub async fn open_connection(
    source: &Source,
    values: &ConnectionValues,
    secrets: &Secrets,
) -> Result<Opened, Unopened> {
    let mut opened = Opened::default();
    let keys = adapter(source).config_requirements();
    for field in keys.iter().copied() {
        let Some(crate::provider::connection::StoredValue::Sealed(stored)) =
            values.get_stored(field.name)
        else {
            continue;
        };
        let value = match stored {
            StoredSecret::Sealed(value) => value,
            StoredSecret::Unreadable { malformed, .. } => {
                return Err(Unopened::Unreadable {
                    field,
                    why: *malformed,
                });
            }
        };
        let binding =
            Binding::of(source, values).map_err(|why| Unopened::Unbound { field, why })?;
        let key = secrets
            .key(value.store())
            .await
            .map_err(|error| Unopened::Store { field, error })?
            .ok_or(Unopened::NoKey { field })?;
        let text = open(value, &key, field, &binding)
            .map_err(|why| Unopened::Unopenable { field, why })?;
        opened.0.insert(field.name, text);
    }
    Ok(opened)
}
