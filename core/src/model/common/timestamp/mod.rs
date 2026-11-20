use std::{cmp::Ordering, fmt};

use chrono::{DateTime, FixedOffset, TimeZone, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// An entry's instant: RFC 3339 text, kept as the source wrote it so it is sent back unchanged,
/// together with the instant it names. Only text that parses makes one, so an entry always has
/// an instant to sort, filter and group by. Sent as its text (`string` in TypeScript).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Timestamp {
    text: String,
    at: DateTime<FixedOffset>,
}

impl Timestamp {
    pub fn parse(text: impl Into<String>) -> Result<Self, chrono::ParseError> {
        let text = text.into();
        let at = DateTime::parse_from_rfc3339(&text)?;
        Ok(Self { text, at })
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// The instant with the offset the source wrote.
    pub fn instant(&self) -> DateTime<FixedOffset> {
        self.at
    }

    pub fn utc(&self) -> DateTime<Utc> {
        self.at.to_utc()
    }
}

/// The instant as `to_rfc3339` writes it, as collectors that compute their instants send them.
impl<Tz: TimeZone> From<DateTime<Tz>> for Timestamp
where
    Tz::Offset: fmt::Display,
{
    fn from(at: DateTime<Tz>) -> Self {
        Self {
            text: at.to_rfc3339(),
            at: at.fixed_offset(),
        }
    }
}

/// Test fixtures write instants as text; one that does not parse is a broken test.
#[cfg(test)]
impl From<&str> for Timestamp {
    fn from(text: &str) -> Self {
        Self::parse(text).unwrap_or_else(|err| panic!("{text:?} is no RFC 3339 instant: {err}"))
    }
}

#[cfg(test)]
impl From<String> for Timestamp {
    fn from(text: String) -> Self {
        Self::from(text.as_str())
    }
}

#[cfg(test)]
impl PartialEq<&str> for Timestamp {
    fn eq(&self, text: &&str) -> bool {
        self.text == *text
    }
}

/// By instant; the same instant written two ways is ordered by its text, so sorting is total.
impl Ord for Timestamp {
    fn cmp(&self, other: &Self) -> Ordering {
        self.at
            .cmp(&other.at)
            .then_with(|| self.text.cmp(&other.text))
    }
}

impl PartialOrd for Timestamp {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

impl Serialize for Timestamp {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.text)
    }
}

impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(text)
            .map_err(|err| serde::de::Error::custom(format!("not an RFC 3339 instant: {err}")))
    }
}

#[cfg(test)]
mod tests;
