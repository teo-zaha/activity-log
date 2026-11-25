use std::{collections::HashSet, hash::Hash};

/// A list repeated a page position or a page of rows already read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RepeatedPage;

/// Tracks one paged read. Call `begin` before paying for and sending each page.
pub(crate) struct PageGuard<P, R> {
    asked: HashSet<P>,
    seen: HashSet<R>,
}

impl<P: Eq + Hash, R: Eq + Hash> PageGuard<P, R> {
    pub(crate) fn new() -> Self {
        Self {
            asked: HashSet::new(),
            seen: HashSet::new(),
        }
    }

    pub(crate) fn begin(&mut self, page: P) -> Result<(), RepeatedPage> {
        self.asked.insert(page).then_some(()).ok_or(RepeatedPage)
    }

    /// Check a position before including it in a batch whose response may fail.
    pub(crate) fn check(&self, page: &P) -> Result<(), RepeatedPage> {
        if self.asked.contains(page) {
            return Err(RepeatedPage);
        }
        Ok(())
    }

    /// Keeps the first occurrence of each row. An empty page can be legitimate; a nonempty
    /// page with nothing new is a repeat even when it was sent for a new position.
    pub(crate) fn new_rows<T>(
        &mut self,
        rows: impl IntoIterator<Item = T>,
        id: impl Fn(&T) -> R,
    ) -> Result<Vec<T>, RepeatedPage> {
        let mut rows = rows.into_iter().peekable();
        let had_rows = rows.peek().is_some();
        let fresh: Vec<_> = rows.filter(|row| self.seen.insert(id(row))).collect();
        if had_rows && fresh.is_empty() {
            return Err(RepeatedPage);
        }
        Ok(fresh)
    }
}
