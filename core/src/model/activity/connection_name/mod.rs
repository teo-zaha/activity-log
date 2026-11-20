use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::model::ConnectionStamp;

/// The connection an entry belongs to, by its name within its kind (`oss`). A collector makes
/// its entries unstamped; the runtime stamps them before an entry is cached or sent. Only a
/// stamped name is ever sent, and only a non-empty one is read, so no entry with an empty
/// connection is stored, sent or accepted (`string` in TypeScript).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct ConnectionName(Option<ConnectionStamp>);

/// An entry was about to be sent before the runtime stamped its connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unstamped;

impl fmt::Display for Unstamped {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an entry is sent only once its connection is stamped")
    }
}

impl ConnectionName {
    /// What a collector's entry carries until the runtime stamps it.
    pub fn unstamped() -> Self {
        Self(None)
    }

    /// Stamps the entry with its connection's name.
    pub fn stamp(&mut self, name: &ConnectionStamp) {
        self.0 = Some(name.clone());
    }

    /// The name once stamped.
    pub fn as_str(&self) -> Option<&str> {
        self.0.as_ref().map(ConnectionStamp::as_str)
    }
}

/// Test fixtures name connections by text; an empty one is a broken test.
#[cfg(test)]
impl From<&str> for ConnectionName {
    fn from(name: &str) -> Self {
        Self(Some(ConnectionStamp::from(name)))
    }
}

#[cfg(test)]
impl PartialEq<&str> for ConnectionName {
    fn eq(&self, name: &&str) -> bool {
        self.as_str() == Some(*name)
    }
}

impl Serialize for ConnectionName {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match &self.0 {
            Some(name) => serializer.serialize_str(name.as_str()),
            None => Err(serde::ser::Error::custom(Unstamped)),
        }
    }
}

impl<'de> Deserialize<'de> for ConnectionName {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let name = String::deserialize(deserializer)?;
        let stamp = ConnectionStamp::new(&name).ok_or_else(|| {
            serde::de::Error::invalid_value(serde::de::Unexpected::Str(""), &"a connection's name")
        })?;
        Ok(Self(Some(stamp)))
    }
}

#[cfg(test)]
mod tests;
