/// A connection's own name, never empty: what the runtime stamps an entry with
/// (`ConnectionName::stamp`), so an entry cannot be stamped with an empty name.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ConnectionStamp(String);

impl ConnectionStamp {
    /// `name` as a stamp; `None` for an empty one.
    pub fn new(name: &str) -> Option<Self> {
        (!name.is_empty()).then(|| Self(name.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Test fixtures name connections by text; an empty one is a broken test.
#[cfg(test)]
impl From<&str> for ConnectionStamp {
    fn from(name: &str) -> Self {
        Self::new(name).expect("a fixture's connection needs a name")
    }
}
