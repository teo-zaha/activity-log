use std::time::Duration;

use tokio::time::Instant;

use super::{Breaker, Budget, STACK_REQUESTS};
use crate::{
    collect::change_stack::lookup::{FailureKind, LookupFailure},
    provider::remote_api::RequestBudget,
};

fn limit() -> LookupFailure {
    FailureKind::Limit.into()
}

fn budget(requests: usize) -> Budget {
    Budget::new(requests, Instant::now() + Duration::from_secs(20))
}

#[tokio::test]
async fn no_more_than_the_limit_is_spent() {
    let budget = budget(2);
    assert_eq!(budget.try_spend(), Ok(()));
    assert_eq!(budget.try_spend(), Ok(()));
    assert_eq!(budget.try_spend(), Err(limit()));
    assert_eq!(budget.spent(), 2);
}

#[tokio::test]
async fn a_phase_never_spends_the_parent_allowance() {
    let stack = Budget::for_collection();
    let projects = stack.phase(20, Duration::from_secs(5));
    for _ in 0..20 {
        assert_eq!(projects.try_spend(), Ok(()));
    }
    assert_eq!(projects.try_spend(), Err(limit()));
    for _ in 0..STACK_REQUESTS {
        assert_eq!(stack.try_spend(), Ok(()));
    }
    assert_eq!(stack.spent(), STACK_REQUESTS);
    assert_eq!(stack.try_spend(), Err(limit()));
}

#[tokio::test(start_paused = true)]
async fn a_phase_ends_at_its_own_deadline_or_the_parent_deadline_if_earlier() {
    for (shared, expected) in [(20, 5), (3, 3)] {
        let start = Instant::now();
        let stack = Budget::new(10, start + Duration::from_secs(shared));
        let projects = stack.phase(10, Duration::from_secs(5));
        let answer = projects
            .run(std::future::pending::<Result<(), LookupFailure>>())
            .await;
        assert_eq!(answer, Err(limit()));
        assert_eq!(start.elapsed(), Duration::from_secs(expected));
        assert_eq!(stack.check().is_ok(), shared > expected);
    }
}

/// Requests reserved from many threads at once never pass the limit.
#[test]
fn requests_are_reserved_atomically() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    let budget = runtime.block_on(async { budget(50) });
    let granted = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..8)
            .map(|_| scope.spawn(|| (0..20).filter(|_| budget.try_spend().is_ok()).count()))
            .collect();
        workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .sum::<usize>()
    });
    assert_eq!((granted, budget.spent()), (50, 50));
}

#[tokio::test(start_paused = true)]
async fn nothing_is_spent_past_the_deadline() {
    let budget = budget(10);
    tokio::time::advance(Duration::from_secs(20)).await;
    assert_eq!(budget.try_spend(), Err(limit()));
    assert_eq!(budget.spent(), 0);
}

/// A call still running at the deadline ends as `Limit`; a call is not started past it.
#[tokio::test(start_paused = true)]
async fn a_call_past_the_deadline_is_a_limit() {
    let budget = budget(10);
    let answer = budget
        .run(std::future::pending::<Result<(), LookupFailure>>())
        .await;
    assert_eq!(answer, Err(limit()));

    let mut started = false;
    let answer = budget
        .run(async {
            started = true;
            Ok(())
        })
        .await;
    assert_eq!(answer, Err(limit()));
    assert!(!started);
}

/// A refused token or a busy host stops every later request with its reason; the first reason
/// stays.
#[tokio::test]
async fn the_breaker_stops_every_later_request() {
    for (kind, breaker) in [
        (FailureKind::TokenInvalid, Breaker::TokenInvalid),
        (FailureKind::ScopeMissing, Breaker::ScopeMissing),
        (FailureKind::Throttled, Breaker::Throttled),
    ] {
        let budget = budget(10);
        let failure = LookupFailure::from(kind);
        assert_eq!(
            budget.run(async { Err::<(), _>(failure.clone()) }).await,
            Err(failure.clone())
        );
        assert_eq!(budget.breaker(), Some(breaker));
        assert_eq!(budget.try_spend(), Err(failure.clone()));
        assert_eq!(budget.check(), Err(failure));

        let other = match kind {
            FailureKind::Throttled => FailureKind::TokenInvalid,
            _ => FailureKind::Throttled,
        };
        budget.trip(other);
        assert_eq!(budget.breaker(), Some(breaker));
    }
}

/// Other failures are the call's own: later requests still go.
#[tokio::test]
async fn other_failures_leave_the_breaker_alone() {
    for kind in [
        FailureKind::Unreadable,
        FailureKind::Limit,
        FailureKind::Failed,
    ] {
        let budget = budget(10);
        let failure = LookupFailure::from(kind);
        assert_eq!(
            budget.run(async { Err::<(), _>(failure.clone()) }).await,
            Err(failure)
        );
        assert_eq!(budget.breaker(), None);
        assert_eq!(budget.try_spend(), Ok(()));
    }
}
