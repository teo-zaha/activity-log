use std::{collections::BTreeSet, fmt};

use reqwest::Url;

use super::config::FigmaConfig;

/// The file or branch Figma's API reads as a single path segment.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct FigmaFileKey(String);

impl FigmaFileKey {
    fn from_key(key: &str) -> Option<Self> {
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
        let invalid = ScopeError::NotAFile { item };
        if input.chars().any(char::is_control) {
            return Err(invalid);
        }

        let url = Url::parse(input).map_err(|_| invalid)?;
        let host = url.host_str().ok_or(invalid)?;
        if !matches!(host, "figma.com" | "www.figma.com") {
            return Err(ScopeError::OtherHost { item });
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
        let key = Self::from_path(&after_scheme[start..]).ok_or(invalid)?;
        (Self::from_path(url.path()).as_ref() == Some(&key))
            .then_some(key)
            .ok_or(invalid)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScopeError {
    Empty,
    NotAFile { item: usize },
    OtherHost { item: usize },
}

impl fmt::Display for ScopeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("Add at least one Figma file link or key."),
            Self::NotAFile { item } => write!(f, "Item {item} is not a Figma file link or key."),
            Self::OtherHost { item } => {
                write!(
                    f,
                    "Item {item} links to another site. Use a link from figma.com."
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
    pub(crate) duplicates: usize,
}

impl FigmaScope {
    pub(crate) fn of(config: &FigmaConfig) -> Result<Self, ScopeError> {
        let items = config
            .files
            .as_deref()
            .filter(|items| !items.is_empty())
            .ok_or(ScopeError::Empty)?;
        let mut seen = BTreeSet::new();
        let mut files = Vec::new();
        for (index, input) in items.iter().enumerate() {
            let position = index + 1;
            let key = FigmaFileKey::from_input(input, position)?;
            if seen.insert(key.clone()) {
                files.push((position, key));
            }
        }
        Ok(Self {
            duplicates: items.len() - files.len(),
            files,
        })
    }
}

#[cfg(test)]
mod tests;
