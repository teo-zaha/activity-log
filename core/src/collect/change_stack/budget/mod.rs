//! What one connection's change stack lookups may spend in one collection: requests, time,
//! and nothing more once the host refused the token or asked to slow down.

use std::{
    sync::{
        OnceLock,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use tokio::time::Instant;

use super::lookup::LookupFailure;
use crate::provider::remote_api::RequestBudget;

/// Requests (pages) one connection's lookups may send in one collection, each reserved before
/// it is sent.
pub(crate) const STACK_REQUESTS: usize = 50;
/// How long one connection's lookups may take in one collection.
pub(crate) const STACK_BUDGET: Duration = Duration::from_secs(20);

/// Why a connection's lookups stopped for good.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Breaker {
    TokenRefused,
    Throttled,
}

impl From<Breaker> for LookupFailure {
    fn from(breaker: Breaker) -> Self {
        match breaker {
            Breaker::TokenRefused => Self::TokenRefused,
            Breaker::Throttled => Self::Throttled,
        }
    }
}

/// One connection's budget for one collection. Shared by all its lookups, which may run at
/// once: requests are reserved atomically, so no more than the limit is ever sent.
#[derive(Debug)]
pub(crate) struct Budget {
    limit: usize,
    spent: AtomicUsize,
    deadline: Instant,
    breaker: OnceLock<Breaker>,
}

impl Budget {
    pub(crate) fn new(requests: usize, deadline: Instant) -> Self {
        Self {
            limit: requests,
            spent: AtomicUsize::new(0),
            deadline,
            breaker: OnceLock::new(),
        }
    }

    /// [`STACK_REQUESTS`] requests within [`STACK_BUDGET`] from now.
    pub(crate) fn for_collection() -> Self {
        Self::new(STACK_REQUESTS, Instant::now() + STACK_BUDGET)
    }

    /// Requests sent so far.
    pub(crate) fn spent(&self) -> usize {
        self.spent.load(Ordering::SeqCst)
    }

    /// Why lookups stopped, once they did.
    pub(crate) fn breaker(&self) -> Option<Breaker> {
        self.breaker.get().copied()
    }

    /// Whether a lookup may still ask: not after the breaker tripped (its reason), nor past the
    /// deadline (`Limit`).
    pub(crate) fn check(&self) -> Result<(), LookupFailure> {
        if let Some(breaker) = self.breaker() {
            return Err(breaker.into());
        }
        if Instant::now() >= self.deadline {
            return Err(LookupFailure::Limit);
        }
        Ok(())
    }

    /// Runs one host call: not at all once [`Self::check`] refuses, `Limit` once the deadline
    /// passes while it runs. A refused token or a busy host trips the breaker, the first
    /// reason kept.
    pub(crate) async fn run<T>(
        &self,
        call: impl Future<Output = Result<T, LookupFailure>>,
    ) -> Result<T, LookupFailure> {
        self.check()?;
        let answer = tokio::time::timeout_at(self.deadline, call)
            .await
            .unwrap_or(Err(LookupFailure::Limit));
        if let Err(failure) = answer {
            self.trip(failure);
        }
        answer
    }

    fn trip(&self, failure: LookupFailure) {
        let breaker = match failure {
            LookupFailure::TokenRefused => Breaker::TokenRefused,
            LookupFailure::Throttled => Breaker::Throttled,
            LookupFailure::Unreadable | LookupFailure::Limit | LookupFailure::Failed => return,
        };
        let _ = self.breaker.set(breaker);
    }
}

impl RequestBudget for Budget {
    type Refusal = LookupFailure;

    /// Reserves one request, or says why none may be sent.
    fn try_spend(&self) -> Result<(), LookupFailure> {
        self.check()?;
        self.spent
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |spent| {
                (spent < self.limit).then_some(spent + 1)
            })
            .map(|_| ())
            .map_err(|_| LookupFailure::Limit)
    }
}

#[cfg(test)]
mod tests;
