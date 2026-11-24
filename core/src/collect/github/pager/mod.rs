use anyhow::Result;
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::Value;

use crate::provider::remote_api::paging::PageGuard;

use super::CollectionError;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Page {
    page_info: PageInfo,
    nodes: Vec<Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageInfo {
    has_next_page: bool,
    end_cursor: Option<String>,
}

pub(super) struct Batch<T> {
    pub(super) rows: Vec<T>,
    pub(super) skipped: usize,
}

enum Position {
    First,
    After(String),
    End,
}

/// A single connection's page positions and row identities.
pub(super) struct Pager {
    position: Position,
    guard: PageGuard<Option<String>, String>,
}

impl Pager {
    pub(super) fn new() -> Self {
        Self {
            position: Position::First,
            guard: PageGuard::new(),
        }
    }

    pub(super) async fn next<T: DeserializeOwned>(
        &mut self,
        fetch: impl AsyncFnOnce(Option<&str>) -> Result<Page>,
        id: impl Fn(&T) -> &str,
    ) -> Result<Option<Batch<T>>> {
        let after = match std::mem::replace(&mut self.position, Position::End) {
            Position::First => None,
            Position::After(cursor) => Some(cursor),
            Position::End => return Ok(None),
        };
        self.guard
            .begin(after.clone())
            .map_err(|_| CollectionError::RepeatedPage)?;
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
            .map_err(|_| CollectionError::RepeatedPage)?;
        if page_info.has_next_page {
            let cursor = page_info
                .end_cursor
                .filter(|cursor| !cursor.trim().is_empty())
                .ok_or(CollectionError::MissingNextCursor)?;
            self.position = Position::After(cursor);
        }
        Ok(Some(Batch { rows, skipped }))
    }
}
