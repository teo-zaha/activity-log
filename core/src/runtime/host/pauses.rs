//! Where a test holds a host's work.

use std::sync::Arc;

use super::Host;
use crate::runtime::lock;

/// Where a test can hold a host's work: each pause signals `reached` and waits for `resume`.
#[derive(Default)]
pub(crate) struct Pauses {
    /// A reload, after its first read of the file.
    pub(crate) after_first_read: Option<Pause>,
    /// An edit, after its write and before its install.
    pub(crate) after_edit_write: Option<Pause>,
}

#[derive(Clone)]
pub(crate) struct Pause {
    pub(crate) reached: Arc<tokio::sync::Semaphore>,
    pub(crate) resume: Arc<tokio::sync::Semaphore>,
}

impl Pause {
    pub(crate) fn new() -> Self {
        Self {
            reached: Arc::new(tokio::sync::Semaphore::new(0)),
            resume: Arc::new(tokio::sync::Semaphore::new(0)),
        }
    }

    async fn hold(&self) {
        self.reached.add_permits(1);
        self.resume.acquire().await.unwrap().forget();
    }
}

impl Host {
    pub(super) async fn pause(&self, which: impl FnOnce(&Pauses) -> Option<Pause>) {
        let pause = which(&lock(&self.inner.pauses));
        if let Some(pause) = pause {
            pause.hold().await;
        }
    }

    pub(crate) fn set_pauses(&self, pauses: Pauses) {
        *lock(&self.inner.pauses) = pauses;
    }
}
