use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::common::source::Source;

/// Identifies one connection: its provider kind and its name within that kind, such as
/// `gitlab` + `oss`. The name never changes with the label, so filters and caches stay put.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ConnectionId {
    pub source: Source,
    pub name: String,
}

impl ConnectionId {
    pub fn new(source: Source, name: impl Into<String>) -> Self {
        Self {
            source,
            name: name.into(),
        }
    }
}

/// `gitlab/oss`, for log lines and the CLI; never stored or sent in this form.
impl fmt::Display for ConnectionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.source.as_ref(), self.name)
    }
}

impl FromStr for ConnectionId {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (kind, name) = value
            .split_once('/')
            .ok_or_else(|| format!("Expected kind/name, such as gitlab/work: {value}"))?;
        let source = kind
            .parse::<Source>()
            .map_err(|_| format!("Unknown provider kind: {kind}"))?;
        if name.is_empty() {
            return Err(format!("Missing connection name in {value}"));
        }
        Ok(Self::new(source, name))
    }
}
