use std::time::Duration;

use reqwest::StatusCode;

use super::*;
use crate::{
    diagnostics,
    provider::source_failure::CollectionIdentityError,
    runtime::{CollectFailure, CollectionKey},
};

#[tokio::test(flavor = "current_thread")]
async fn a_collection_refusal_publishes_with_eviction_once_and_recheck_recovers() {
    let logs = diagnostics::LocalLog::start();
    let id = ConnectionId::new(Source::Jira, "denial-pair");
    let sibling = gitlab();
    let world = World::shared();
    world.name(&id, "ada");
    let (host, mut spawned) = host_of(&world, &[connection(&id, &[]), connection(&sibling, &[])]);
    host.collect(march());
    spawned.end().await;
    assert_eq!(
        labels(&host.activity()),
        ["denial-pair as ada", "work as nobody"]
    );
    let verified = host.lock().keys.key(&id).unwrap();

    let gate = world.hold_collections(&sibling);
    world.remote_failures.lock().unwrap().insert(
        id.clone(),
        http_error(StatusCode::UNAUTHORIZED, Some(OAuthRefusal::InvalidToken)),
    );
    host.collect(march());
    let before = host.subscribe().borrow().clone();
    let denied = tokio::time::timeout(
        Duration::from_secs(5),
        until(&host, |published| {
            matches!(&*published.activity.value, ActivityState::Running { log }
            if log.meta.pending == [sibling.clone()])
        }),
    )
    .await
    .unwrap();

    let info = &denied.providers.value.providers[0];
    assert_eq!(info.connection, id);
    assert_eq!(info.status, ProviderStatus::Unavailable);
    assert_eq!(info.problem, Some(ProviderProblem::Refused));
    assert!(!info.usable && info.recheckable);
    assert_eq!(labels(&denied.activity.value), ["work as nobody"]);
    assert_eq!(denied.authority, before.authority + 1);
    assert_eq!(denied.activity.revision, denied.providers.revision);
    let refused_key = host.lock().keys.key(&id).unwrap();
    assert_ne!(refused_key, verified);

    gate.add_permits(1);
    spawned.end().await;
    let settled = host.subscribe().borrow().clone();
    assert_eq!(settled.authority, denied.authority);
    assert_eq!(settled.providers.revision, denied.providers.revision);
    assert_eq!(host.lock().keys.key(&id), Some(refused_key));
    assert_eq!(labels(&settled.activity.value), ["work as nobody"]);
    // diagnostics::record asserts the state lock is released. Check that the warning ran once.
    assert_eq!(
        logs.entries()
            .iter()
            .filter(|line| {
                line.scope == id.to_string() && line.level == crate::model::LogLevel::Warn
            })
            .count(),
        1
    );

    world.remote_failures.lock().unwrap().remove(&id);
    host.recheck(&id).unwrap();
    spawned.end().await;
    let recovered = host.subscribe().borrow().clone();
    assert_eq!(recovered.authority, settled.authority + 1);
    assert_eq!(status(&host, &id), ProviderStatus::Available);
    let restored = host.lock().keys.key(&id).unwrap();
    assert_ne!(restored, refused_key);
    assert_ne!(restored, verified);
    assert_eq!(
        labels(&recovered.activity.value),
        ["denial-pair as ada", "work as nobody"]
    );
}

#[tokio::test]
async fn collection_observes_another_account_and_the_next_check_uses_its_key() {
    let id = ConnectionId::new(Source::Figma, "studio");
    let world = World::shared();
    world.name(&id, "ada");
    let (host, mut spawned) = host_of(&world, &[connection(&id, &[])]);
    host.collect(march());
    spawned.end().await;
    assert_eq!(labels(&host.activity()), ["studio as ada"]);
    let original = host.lock().keys.key(&id).unwrap();

    let gate = world.hold_collections(&id);
    host.collect(march());
    let before = host.subscribe().borrow().clone();
    let run = shown_run(&host).unwrap();
    let changed = CollectionIdentityError::Changed(RemoteIdentity {
        id: "bea".into(),
        username: "bea".into(),
    });
    assert_eq!(
        host.finish(run, march(), Ok(failed(&id, original, changed.into()))),
        RunOutcome::Ready { entries: 0 }
    );
    let denied = host.subscribe().borrow().clone();
    assert_eq!(denied.authority, before.authority + 1);
    assert_eq!(denied.activity.revision, denied.providers.revision);
    assert_eq!(
        denied.providers.value.providers[0].problem,
        Some(ProviderProblem::Refused)
    );
    assert!(labels(&denied.activity.value).is_empty());
    let new_key = host.lock().keys.key(&id).unwrap();
    assert_ne!(new_key, original);

    gate.add_permits(1);
    assert_eq!(spawned.end().await.outcome, RunOutcome::Stopped);
    world.name(&id, "bea");
    host.recheck(&id).unwrap();
    gate.add_permits(1);
    spawned.end().await;
    assert_eq!(host.lock().keys.key(&id), Some(new_key));
    assert_eq!(host.subscribe().borrow().authority, denied.authority);
    assert_eq!(labels(&host.activity()), ["studio as bea"]);
}

#[tokio::test]
async fn a_first_account_denial_records_the_key_without_raising_authority() {
    let id = jira();
    let world = World::shared();
    world
        .remote_failures
        .lock()
        .unwrap()
        .insert(id.clone(), http_error(StatusCode::UNAUTHORIZED, None));
    let (host, mut spawned) = host_of(&world, &[connection(&id, &[])]);
    let checked = until(&host, checked).await;
    let original = host.lock().keys.key(&id).unwrap();

    host.collect(march());
    spawned.end().await;

    let denied = host.subscribe().borrow().clone();
    assert_eq!(denied.authority, checked.authority);
    assert_ne!(host.lock().keys.key(&id), Some(original));
    assert_eq!(status(&host, &id), ProviderStatus::Unavailable);
    assert_eq!(
        denied.providers.value.providers[0].problem,
        Some(ProviderProblem::Refused)
    );
}

#[tokio::test]
async fn resource_refusals_and_temporary_failures_keep_the_key_and_entries() {
    let cases = [
        (
            Source::Jira,
            http_error(StatusCode::TOO_MANY_REQUESTS, None),
            ProviderProblem::Throttled,
        ),
        (
            Source::Jira,
            ApiError::Unreachable,
            ProviderProblem::Unreachable,
        ),
        (
            Source::Gitlab,
            http_error(
                StatusCode::UNAUTHORIZED,
                Some(OAuthRefusal::InsufficientScope),
            ),
            ProviderProblem::ScopeMissing,
        ),
        (
            Source::Jira,
            http_error(StatusCode::FORBIDDEN, None),
            ProviderProblem::Refused,
        ),
        (
            Source::Github,
            http_error(StatusCode::FORBIDDEN, None),
            ProviderProblem::Refused,
        ),
        (
            Source::Gitlab,
            http_error(StatusCode::FORBIDDEN, None),
            ProviderProblem::Refused,
        ),
        (
            Source::Figma,
            http_error(StatusCode::FORBIDDEN, None),
            ProviderProblem::Refused,
        ),
    ];
    for (source, error, expected) in cases {
        let id = ConnectionId::new(source, "work");
        let world = World::shared();
        world.name(&id, "ada");
        let (host, mut spawned) = host_of(&world, &[connection(&id, &[])]);
        host.collect(march());
        spawned.end().await;
        let original = host.subscribe().borrow().clone();
        let key = host.lock().keys.key(&id).unwrap();
        world
            .remote_failures
            .lock()
            .unwrap()
            .insert(id.clone(), error);
        for _ in 0..2 {
            host.collect(march());
            spawned.end().await;
            let failed = host.subscribe().borrow().clone();
            let info = &failed.providers.value.providers[0];
            assert_eq!(info.status, ProviderStatus::Degraded, "{id}");
            assert_eq!(info.problem, Some(expected), "{id}");
            assert_eq!(failed.authority, original.authority, "{id}");
            assert_eq!(host.lock().keys.key(&id), Some(key), "{id}");
            assert_eq!(labels(&failed.activity.value), ["work as ada"], "{id}");
        }
    }
}

#[tokio::test]
async fn an_unreadable_remote_response_keeps_entries_but_a_generic_failure_does_not() {
    let id = jira();
    let world = World::shared();
    world.name(&id, "ada");
    let (host, mut spawned) = host_of(&world, &[connection(&id, &[])]);
    host.collect(march());
    spawned.end().await;
    let key = host.lock().keys.key(&id).unwrap();
    let authority = host.subscribe().borrow().authority;

    for (error, entries) in [
        (ApiError::Decode, vec!["work as ada"]),
        (http_error(StatusCode::INTERNAL_SERVER_ERROR, None), vec![]),
    ] {
        world
            .remote_failures
            .lock()
            .unwrap()
            .insert(id.clone(), error);
        host.collect(march());
        spawned.end().await;
        let failed = host.subscribe().borrow().clone();
        assert_eq!(
            failed.providers.value.providers[0].problem,
            Some(ProviderProblem::CollectionFailed)
        );
        assert_eq!(failed.authority, authority);
        assert_eq!(host.lock().keys.key(&id), Some(key));
        assert_eq!(labels(&failed.activity.value), entries);
    }
}

#[tokio::test(start_paused = true)]
async fn an_expired_collection_deadline_keeps_entries_under_the_same_key() {
    let id = jira();
    let world = World::shared();
    world.name(&id, "ada");
    let (host, mut spawned) = host_of(&world, &[connection(&id, &[])]);
    host.collect(march());
    spawned.end().await;
    let key = host.lock().keys.key(&id).unwrap();
    let authority = host.subscribe().borrow().authority;
    world.hold_collections(&id);

    host.collect(march());
    spawned.end().await;

    let failed = host.subscribe().borrow().clone();
    assert_eq!(
        failed.providers.value.providers[0].problem,
        Some(ProviderProblem::TimedOut)
    );
    assert_eq!(failed.authority, authority);
    assert_eq!(host.lock().keys.key(&id), Some(key));
    assert_eq!(labels(&failed.activity.value), ["work as ada"]);
}

#[tokio::test]
async fn a_denial_from_before_the_current_key_or_from_after_shutdown_cannot_replace_it() {
    let id = jira();
    let world = World::shared();
    world.name(&id, "ada");
    let (host, mut spawned) = host_of(&world, &[connection(&id, &[])]);
    host.collect(march());
    spawned.end().await;
    let old_key = host.lock().keys.key(&id).unwrap();
    world.name(&id, "bea");
    world.hold_collections(&id);
    host.recheck(&id).unwrap();
    until(&host, checked).await;
    let key = host.lock().keys.key(&id).unwrap();
    assert_ne!(key, old_key);
    let run = shown_run(&host).unwrap();
    let before = host.subscribe().borrow().clone();
    let refused = || {
        RemoteFailure {
            service: "Jira".into(),
            error: http_error(StatusCode::UNAUTHORIZED, None),
        }
        .into()
    };

    host.progress(run, march(), failed(&id, old_key, refused()));
    assert_eq!(host.lock().keys.key(&id), Some(key));
    assert_eq!(status(&host, &id), ProviderStatus::Available);
    assert_eq!(host.subscribe().borrow().authority, before.authority);

    host.shutdown();
    let revision = host.subscribe().borrow().revision();
    let denial = failed(&id, key, refused());
    host.progress(run, march(), denial.clone());
    assert_eq!(host.finish(run, march(), Ok(denial)), RunOutcome::Stopped);
    assert_eq!(host.lock().keys.key(&id), Some(key));
    assert_eq!(status(&host, &id), ProviderStatus::Available);
    assert_eq!(host.subscribe().borrow().revision(), revision);
}

fn failed(id: &ConnectionId, key: CollectionKey, error: anyhow::Error) -> RunReport {
    RunReport {
        slots: vec![(
            id.clone(),
            Slot::Settled(Outcome::Failed {
                key,
                failure: CollectFailure::of(&error),
                carried: None,
            }),
        )],
    }
}
