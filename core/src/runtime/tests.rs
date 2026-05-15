use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use tokio::{
    sync::Notify,
    time::{Duration, sleep},
};

use crate::{
    model::{
        ActivityLog, ActivityLogMeta, CollectOptions, CollectRequest, CollectedActivitySummary,
        ProviderInfo, ProviderStatus, Source, SupportedOs,
    },
    runtime::{
        ActivitySnapshot, CollectionResult, CollectionRuntime, Collector, CollectorFuture,
        resolve_collect_options,
    },
};

fn fixture_log() -> ActivityLog {
    ActivityLog {
        meta: ActivityLogMeta {
            generated: "2026-03-25T10:00:00Z".to_string(),
            since: "2026-03-01".to_string(),
            until: "2026-03-25".to_string(),
        },
        summary: CollectedActivitySummary::default(),
        entries: Vec::new(),
    }
}

fn fixture_options() -> CollectOptions {
    CollectOptions {
        since: chrono::NaiveDate::from_ymd_opt(2026, 3, 1).unwrap(),
        until: chrono::NaiveDate::from_ymd_opt(2026, 3, 25).unwrap(),
    }
}

fn fixture_providers() -> Vec<ProviderInfo> {
    vec![ProviderInfo {
        id: Source::Git,
        status: ProviderStatus::Available,
        usable: true,
        supported_os: vec![SupportedOs::Windows, SupportedOs::Macos, SupportedOs::Linux],
        reason: None,
        checked_at: "2026-03-25T10:00:00Z".to_string(),
    }]
}

fn collector_ok() -> Collector {
    Arc::new(|_options: CollectOptions| -> CollectorFuture {
        Box::pin(async { Ok(fixture_log()) })
    })
}

#[test]
fn initial_collection_task_can_be_created_without_runtime() {
    let runtime = CollectionRuntime::new(collector_ok(), fixture_providers());

    assert!(runtime.initial_collection_task().is_some());
}

#[tokio::test]
async fn activity_snapshot_is_empty_without_cache() {
    let runtime = CollectionRuntime::new(collector_ok(), fixture_providers());

    assert!(matches!(
        runtime.activity_snapshot().await,
        ActivitySnapshot::Empty
    ));
}

#[tokio::test]
async fn collect_returns_collecting_when_busy() {
    let started = Arc::new(AtomicBool::new(false));
    let release = Arc::new(Notify::new());
    let started_for_collector = Arc::clone(&started);
    let release_for_collector = Arc::clone(&release);
    let collector: Collector = Arc::new(move |_options: CollectOptions| -> CollectorFuture {
        let started = Arc::clone(&started_for_collector);
        let release = Arc::clone(&release_for_collector);
        Box::pin(async move {
            started.store(true, Ordering::SeqCst);
            release.notified().await;
            Ok(fixture_log())
        })
    });
    let runtime = CollectionRuntime::new(collector, fixture_providers());
    let runtime_for_task = runtime.clone();

    let first = tokio::spawn(async move {
        runtime_for_task
            .collect_with_options(fixture_options())
            .await
    });

    for _ in 0..20 {
        if started.load(Ordering::SeqCst) {
            break;
        }
        sleep(Duration::from_millis(10)).await;
    }

    let second = runtime
        .collect_with_options(fixture_options())
        .await
        .unwrap();
    assert!(matches!(second, CollectionResult::Collecting));

    release.notify_one();
    let first = first.await.unwrap().unwrap();
    assert!(matches!(first, CollectionResult::Ready(_)));
}

#[tokio::test]
async fn spawn_initial_collection_populates_cache() {
    let runtime = CollectionRuntime::new(collector_ok(), fixture_providers());

    let task = runtime
        .initial_collection_task()
        .expect("initial collection task");
    tokio::spawn(task);

    for _ in 0..20 {
        if matches!(
            runtime.activity_snapshot().await,
            ActivitySnapshot::Ready(_)
        ) {
            return;
        }
        sleep(Duration::from_millis(10)).await;
    }

    panic!("expected initial collection to populate the cache");
}

#[test]
fn resolve_collect_options_rejects_mixed_modes() {
    let request = CollectRequest {
        since: Some("2026-03-01".to_string()),
        until: Some("2026-03-05".to_string()),
        preset: Some(crate::model::RangePreset::Today),
    };

    let result = resolve_collect_options(&request);
    assert!(matches!(
        result,
        Err("Provide either since/until or preset, not both")
    ));
}

#[test]
fn resolve_collect_options_accepts_preset() {
    let request = CollectRequest {
        since: None,
        until: None,
        preset: Some(crate::model::RangePreset::Today),
    };

    let options = resolve_collect_options(&request).unwrap();
    assert!(options.since <= options.until);
}
