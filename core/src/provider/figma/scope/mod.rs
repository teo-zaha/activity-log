use std::{collections::BTreeSet, fmt};

use reqwest::Url;
use serde::{Deserialize, Deserializer, Serialize};

use super::config::FigmaConfig;

mod team_id;
pub(crate) use team_id::FigmaTeamId;

/// The file or branch Figma's API reads as a single path segment.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub(crate) struct FigmaFileKey(String);

impl<'de> Deserialize<'de> for FigmaFileKey {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let key = String::deserialize(deserializer)?;
        Self::from_key(&key).ok_or_else(|| serde::de::Error::custom("invalid Figma file key"))
    }
}

impl FigmaFileKey {
    pub(crate) fn from_key(key: &str) -> Option<Self> {
        (!key.is_empty()
            && key.len() <= 128
            && key.bytes().all(|byte| byte.is_ascii_alphanumeric()))
        .then(|| Self(key.to_owned()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    /// A path from a pasted link or a desktop tab, with the branch key when it names a branch.
    pub(crate) fn from_path(path: &str) -> Option<Self> {
        let path = path.split(['?', '#']).next()?;
        if path.contains('\\') || path.chars().any(char::is_control) {
            return None;
        }

        let mut segments = path.strip_prefix('/')?.split('/');
        if !matches!(
            segments.next()?,
            "file" | "design" | "board" | "slides" | "proto" | "make" | "site"
        ) {
            return None;
        }

        let file = Self::from_key(segments.next()?)?;
        match segments.next() {
            Some("branch") => Self::from_key(segments.next()?),
            _ => Some(file),
        }
    }

    fn from_input(input: &str, item: usize) -> Result<Self, ScopeError> {
        if let Some(key) = Self::from_key(input) {
            return Ok(key);
        }
        from_link(input, ScopeError::NotAFile { item }, Self::from_path)
    }
}

fn from_link<T: PartialEq>(
    input: &str,
    invalid: ScopeError,
    from_path: impl Fn(&str) -> Option<T>,
) -> Result<T, ScopeError> {
    if input.chars().any(char::is_control) {
        return Err(invalid);
    }
    let url = Url::parse(input).map_err(|_| invalid)?;
    let host = url.host_str().ok_or(invalid)?;
    if !matches!(host, "figma.com" | "www.figma.com") {
        return Err(match invalid {
            ScopeError::NotAFile { item } => ScopeError::OtherHost { item },
            _ => invalid,
        });
    }
    if url.scheme() != "https"
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(invalid);
    }

    // Read keys from the original path so URL normalization cannot erase an invalid key.
    let (_, after_scheme) = input.split_once("://").ok_or(invalid)?;
    let start = after_scheme.find(['/', '?', '#', '\\']).ok_or(invalid)?;
    let key = from_path(&after_scheme[start..]).ok_or(invalid)?;
    (from_path(url.path()).as_ref() == Some(&key))
        .then_some(key)
        .ok_or(invalid)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScopeError {
    NotAFile { item: usize },
    OtherHost { item: usize },
    NotATeam { item: usize },
}

impl fmt::Display for ScopeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAFile { item } => write!(f, "Item {item} is not a Figma file link or key."),
            Self::OtherHost { item } => {
                write!(
                    f,
                    "Item {item} links to another site. Use a link from figma.com."
                )
            }
            Self::NotATeam { item } => {
                write!(
                    f,
                    "Figma team list item {item} must be a team link from figma.com or a numeric team ID."
                )
            }
        }
    }
}

impl std::error::Error for ScopeError {}

#[derive(Debug)]
pub(crate) struct FigmaScope {
    /// The original one-based list positions give people a way to identify failed files.
    pub(crate) files: Vec<(usize, FigmaFileKey)>,
    pub(crate) teams: Vec<(usize, FigmaTeamId)>,
    pub(crate) duplicate_files: usize,
    pub(crate) duplicate_teams: usize,
}

impl FigmaScope {
    pub(crate) fn of(config: &FigmaConfig) -> Result<Self, ScopeError> {
        let file_items = config.files.as_deref().unwrap_or_default();
        let team_items = config.teams.as_deref().unwrap_or_default();
        let files = distinct_items(file_items, FigmaFileKey::from_input)?;
        let teams = distinct_items(team_items, FigmaTeamId::from_input)?;
        Ok(Self {
            duplicate_files: file_items.len() - files.len(),
            duplicate_teams: team_items.len() - teams.len(),
            files,
            teams,
        })
    }
}

/// Keep the original position of the first occurrence of each parsed item.
fn distinct_items<T: Ord + Clone>(
    items: &[String],
    parse: impl Fn(&str, usize) -> Result<T, ScopeError>,
) -> Result<Vec<(usize, T)>, ScopeError> {
    let mut seen = BTreeSet::new();
    let mut distinct = Vec::new();
    for (index, input) in items.iter().enumerate() {
        let position = index + 1;
        let item = parse(input, position)?;
        if seen.insert(item.clone()) {
            distinct.push((position, item));
        }
    }
    Ok(distinct)
}

#[cfg(test)]
mod tests;
