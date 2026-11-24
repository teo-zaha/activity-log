//! Reading a paged GitLab list: pages follow GitLab's pagination headers, each is paid for
//! before it is sent, and a list that comes back to a page or rows it already sent stops.

use reqwest::Url;
use serde::de::DeserializeOwned;

use super::GitLabApi;
use crate::provider::{
    remote_api::{RequestBudget, Retry, paging::PageGuard},
    remote_error::ApiError,
};

/// A row of a paged list, known by the id GitLab gives it.
pub(crate) trait PagedRow: DeserializeOwned {
    fn row_id(&self) -> u64;
}

/// The rows of one page not read before, and how many rows were not of the expected shape.
#[derive(Debug)]
pub(crate) struct Page<T> {
    pub rows: Vec<T>,
    pub skipped: usize,
}

/// Why a list was not read to its end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PageError<R> {
    /// A page's request failed, or its answer is no JSON list.
    Request(ApiError),
    /// GitLab named a page it had already sent, or sent only rows already read: read on, the
    /// list would never end.
    Repeated,
    /// The budget allowed no further request.
    Refused(R),
}

/// One paged list, read a page at a time from its first page's URL.
pub(crate) struct Pager<'a, B: RequestBudget> {
    api: &'a GitLabApi,
    budget: &'a B,
    retry: Retry,
    next: Option<Url>,
    guard: PageGuard<Url, u64>,
}

impl<'a, B: RequestBudget> Pager<'a, B> {
    pub(crate) fn new(api: &'a GitLabApi, first: Url, retry: Retry, budget: &'a B) -> Self {
        Self {
            api,
            budget,
            retry,
            next: Some(first),
            guard: PageGuard::new(),
        }
    }

    /// Whether GitLab named a page after the ones read: a list read only in part (a first
    /// page) tells by it whether it is the whole list.
    pub(crate) fn has_next(&self) -> bool {
        self.next.is_some()
    }

    /// The next page, `None` past the last. After an error there is none.
    pub(crate) async fn next_page<T: PagedRow>(
        &mut self,
    ) -> Option<Result<Page<T>, PageError<B::Refusal>>> {
        let url = self.next.take()?;
        Some(self.read(url).await)
    }

    async fn read<T: PagedRow>(&mut self, url: Url) -> Result<Page<T>, PageError<B::Refusal>> {
        self.guard
            .begin(url.clone())
            .map_err(|_| PageError::Repeated)?;
        self.budget.try_spend().map_err(PageError::Refused)?;
        // A page that is no JSON list fails the read; a row that is no `T` is skipped.
        let (values, following) = self
            .api
            .get_page::<Vec<serde_json::Value>>(url, self.retry)
            .await
            .map_err(PageError::Request)?;
        let mut skipped = 0;
        let parsed =
            values
                .into_iter()
                .filter_map(|value| match serde_json::from_value::<T>(value) {
                    Ok(row) => Some(row),
                    Err(_) => {
                        skipped += 1;
                        None
                    }
                });
        let rows = self
            .guard
            .new_rows(parsed, T::row_id)
            .map_err(|_| PageError::Repeated)?;
        self.next = following;
        Ok(Page { rows, skipped })
    }
}

#[cfg(test)]
mod tests;
