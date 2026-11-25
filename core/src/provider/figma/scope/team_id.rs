use serde::{Deserialize, Serialize};

use super::{ScopeError, from_link};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct FigmaTeamId(#[serde(deserialize_with = "deserialize_id")] String);

impl FigmaTeamId {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    pub(super) fn from_input(input: &str, item: usize) -> Result<Self, ScopeError> {
        if is_id(input) {
            return Ok(Self(input.to_owned()));
        }
        from_link(input, ScopeError::NotATeam { item }, Self::from_path)
    }

    fn from_path(path: &str) -> Option<Self> {
        let path = path.split(['?', '#']).next()?;
        if path.contains('\\') {
            return None;
        }
        let mut parts = path.strip_prefix("/files/")?.split('/');
        let first = parts.next()?;
        if first != "team" {
            if !is_id(first) {
                return None;
            }
            parts.next().filter(|part| *part == "team")?;
        }
        let id = parts.next().filter(|part| is_id(part))?;
        Some(Self(id.to_owned()))
    }
}

fn is_id(id: &str) -> bool {
    !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit())
}

fn deserialize_id<'de, D: serde::Deserializer<'de>>(de: D) -> Result<String, D::Error> {
    let id = String::deserialize(de)?;
    if is_id(&id) {
        Ok(id)
    } else {
        Err(serde::de::Error::custom("invalid Figma team ID"))
    }
}
