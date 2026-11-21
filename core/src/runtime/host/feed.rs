//! One subscriber's view of the published state.

use std::sync::Arc;

use tokio::sync::watch;

use super::{Host, Published};

/// One subscriber's view of the published state, from [`Host::feed`]: the current value at
/// once, then the latest one after each change (values in between are skipped, never
/// reordered), until the host shuts down.
pub struct Feed {
    values: watch::Receiver<Arc<Published>>,
    stop: watch::Receiver<bool>,
}

impl Feed {
    /// The latest value not yet returned, waiting for one; `None` once the host shut down or is
    /// gone. The value is cloned out of the watch, so no lock is held while it is serialized.
    pub async fn next(&mut self) -> Option<Arc<Published>> {
        loop {
            if *self.stop.borrow_and_update() {
                return None;
            }
            if self.values.has_changed().ok()? {
                return Some(self.values.borrow_and_update().clone());
            }
            tokio::select! {
                changed = self.values.changed() => {
                    changed.ok()?;
                    // Taken at the top, after a shutdown at the same moment had its say.
                    self.values.mark_changed();
                }
                stopped = self.stop.changed() => stopped.ok()?,
            }
        }
    }
}

impl Host {
    /// The published state for one subscriber that ends at shutdown: what the event stream and
    /// the desktop's events forward.
    pub fn feed(&self) -> Feed {
        let mut values = self.subscribe();
        // The current value is the first one returned.
        values.mark_changed();
        Feed {
            values,
            stop: self.inner.shutdown.subscribe(),
        }
    }
}
