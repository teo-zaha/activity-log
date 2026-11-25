use serde::{Deserialize, de::DeserializeOwned};
use serde_json::Value;

use crate::provider::remote_api::paging::PageGuard;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PageError {
    RepeatedPage,
    MissingNextCursor,
}

impl std::fmt::Display for PageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::RepeatedPage => "GitHub repeated a page of results",
            Self::MissingNextCursor => "GitHub did not name the next page of results",
        })
    }
}

impl std::error::Error for PageError {}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Page {
    page_info: PageInfo,
    nodes: Vec<Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageInfo {
    has_next_page: bool,
    end_cursor: Option<String>,
}

pub(crate) struct Batch<T> {
    pub(crate) rows: Vec<T>,
    pub(crate) skipped: usize,
}

enum Position {
    First,
    After(String),
    End,
}

/// A single connection's page positions and row identities.
pub(crate) struct Pager {
    position: Position,
    guard: PageGuard<Option<String>, String>,
}

impl Pager {
    pub(crate) fn new() -> Self {
        Self {
            position: Position::First,
            guard: PageGuard::new(),
        }
    }

    pub(crate) fn has_next(&self) -> bool {
        matches!(self.position, Position::After(_))
    }

    pub(crate) async fn next<T: DeserializeOwned, E: From<PageError>>(
        &mut self,
        fetch: impl AsyncFnOnce(Option<&str>) -> Result<Page, E>,
        id: impl Fn(&T) -> &str,
    ) -> Result<Option<Batch<T>>, E> {
        let after = match std::mem::replace(&mut self.position, Position::End) {
            Position::First => None,
            Position::After(cursor) => Some(cursor),
            Position::End => return Ok(None),
        };
        self.guard
            .begin(after.clone())
            .map_err(|_| PageError::RepeatedPage)?;
        let Page { page_info, nodes } = fetch(after.as_deref()).await?;
        let mut skipped = 0;
        let read = nodes
            .into_iter()
            .filter_map(|node| match serde_json::from_value(node) {
                Ok(row) => Some(row),
                Err(_) => {
                    skipped += 1;
                    None
                }
            });
        let rows = self
            .guard
            .new_rows(read, |row| id(row).to_owned())
            .map_err(|_| PageError::RepeatedPage)?;
        if page_info.has_next_page {
            let cursor = page_info
                .end_cursor
                .filter(|cursor| !cursor.trim().is_empty())
                .ok_or(PageError::MissingNextCursor)?;
            self.position = Position::After(cursor);
        }
        Ok(Some(Batch { rows, skipped }))
    }
}

#[cfg(test)]
mod tests;
