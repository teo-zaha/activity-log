//! What one connection's change stack lookups may spend in one collection: requests, time,
//! and nothing more once the host refused the token or asked to slow down.

use std::{
    sync::{
        Arc, OnceLock,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use tokio::time::Instant;

use super::lookup::{FailureKind, LookupFailure};
use crate::provider::remote_api::RequestBudget;

/// Requests (pages) one connection's stack lookups may send in one collection, each reserved before
/// it is sent.
pub(crate) const STACK_REQUESTS: usize = 50;
/// How long one connection's lookups may take in one collection.
pub(crate) const STACK_BUDGET: Duration = Duration::from_secs(20);

/// Why a connection's lookups stopped for good.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Breaker {
    TokenInvalid,
    ScopeMissing,
    Throttled,
}

impl Breaker {
    /// The breaker a failure of `kind` trips, if any: one that holds for every later request.
    fn of(kind: FailureKind) -> Option<Self> {
        match kind {
            FailureKind::TokenInvalid => Some(Self::TokenInvalid),
            FailureKind::ScopeMissing => Some(Self::ScopeMissing),
            FailureKind::Throttled => Some(Self::Throttled),
            FailureKind::Unreadable | FailureKind::Limit | FailureKind::Failed => None,
        }
    }
}

impl From<Breaker> for FailureKind {
    fn from(breaker: Breaker) -> Self {
        match breaker {
            Breaker::TokenInvalid => Self::TokenInvalid,
            Breaker::ScopeMissing => Self::ScopeMissing,
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
    breaker: Arc<OnceLock<Breaker>>,
}

impl Budget {
    pub(crate) fn new(requests: usize, deadline: Instant) -> Self {
        Self {
            limit: requests,
            spent: AtomicUsize::new(0),
            deadline,
            breaker: Arc::default(),
        }
    }

    /// [`STACK_REQUESTS`] stack requests within the shared [`STACK_BUDGET`] annotation deadline.
    pub(crate) fn for_collection() -> Self {
        Self::new(STACK_REQUESTS, Instant::now() + STACK_BUDGET)
    }

    /// Gives a phase its own request allowance and a deadline bounded by this one's. The
    /// breaker still stops both budgets when the host refuses the token or asks to slow down.
    pub(crate) fn phase(&self, requests: usize, time: Duration) -> Self {
        Self {
            limit: requests,
            spent: AtomicUsize::new(0),
            deadline: self.deadline.min(Instant::now() + time),
            breaker: self.breaker.clone(),
        }
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
            return Err(FailureKind::from(breaker).into());
        }
        if Instant::now() >= self.deadline {
            return Err(FailureKind::Limit.into());
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
            .unwrap_or_else(|_| Err(FailureKind::Limit.into()));
        if let Err(failure) = &answer {
            self.trip(failure.kind());
        }
        answer
    }

    fn trip(&self, kind: FailureKind) {
        if let Some(breaker) = Breaker::of(kind) {
            let _ = self.breaker.set(breaker);
        }
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
            .map_err(|_| FailureKind::Limit.into())
    }
}

#[cfg(test)]
mod tests;
