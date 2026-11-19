use std::sync::{Arc, Mutex};

use tokio::{
    sync::Notify,
    time::{Duration, sleep},
};

use crate::{
    model::{
        ActivityEntry, ActivityLog, ActivityLogMeta, CollectOptions, CollectRequest,
        CollectedActivitySummary, ConnectionCount, ConnectionId, GitEntry, ProviderInfo,
        ProviderStatus, Source, SupportedOs,
    },
    provider::connection::{Connection, ConnectionLocation, ConnectionValues},
    runtime::{
        ActivitySnapshot, Carried, CollectionRuntime, Collector, CollectorFuture, Progress,
        kept_except, resolve_collect_options,
    },
};

fn fixture_log() -> ActivityLog {
    ActivityLog {
        meta: ActivityLogMeta {
            generated: "2026-03-25T10:00:00Z".to_string(),
            start: "2026-03-01T00:00:00+00:00".to_string(),
            end: "2026-03-26T00:00:00+00:00".to_string(),
            pending: Vec::new(),
        },
        summary: CollectedActivitySummary::default(),
        entries: Vec::new(),
    }
}

/// What a collector reports once Git is in and GitLab is still collecting.
fn partial_log() -> ActivityLog {
    let mut log = fixture_log();
    log.meta.pending = vec![ConnectionId::new(Source::Gitlab, "default")];
    log.entries = vec![ActivityEntry::Git(GitEntry::new(
        "repo".to_string(),
        "abc123".to_string(),
        "message".to_string(),
        "me@example.com".to_string(),
        "main".to_string(),
        "2026-03-10T10:00:00Z".to_string(),
    ))];
    log
}

fn fixture_options() -> CollectOptions {
    use chrono::TimeZone;
    CollectOptions {
        start: chrono::Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap(),
        end: chrono::Utc.with_ymd_and_hms(2026, 3, 26, 0, 0, 0).unwrap(),
    }
}

fn other_options() -> CollectOptions {
    CollectOptions {
        start: fixture_options().start - chrono::Duration::days(1),
        ..fixture_options()
    }
}

fn fixture_providers() -> Vec<ProviderInfo> {
    vec![ProviderInfo {
        connection: ConnectionId::new(Source::Git, "default"),
        label: "Git".to_string(),
        wsl_distro: None,
        status: ProviderStatus::Available,
        usable: true,
        supported_os: vec![SupportedOs::Windows, SupportedOs::Macos, SupportedOs::Linux],
        reason: None,
        problem: None,
        checked_at: "2026-03-25T10:00:00Z".to_string(),
    }]
}

fn collector_ok() -> Collector {
    Arc::new(
        |_options: CollectOptions, _carried, _scope, _progress: Progress| -> CollectorFuture {
            Box::pin(async { Ok(fixture_log()) })
        },
    )
}

/// A collector that hands out its progress callback and finishes once `release` is notified.
struct HeldCollector {
    progress: Arc<Mutex<Option<Progress>>>,
    release: Arc<Notify>,
    /// The range of each run the collector was asked for, in order.
    ranges: Arc<Mutex<Vec<CollectOptions>>>,
}

impl HeldCollector {
    fn new() -> (Self, Collector) {
        let held = Self {
            progress: Arc::new(Mutex::new(None)),
            release: Arc::new(Notify::new()),
            ranges: Arc::new(Mutex::new(Vec::new())),
        };
        let (slot, release, ranges) = (
            held.progress.clone(),
            held.release.clone(),
            held.ranges.clone(),
        );
        let collector: Collector = Arc::new(
            move |options: CollectOptions, _carried, _scope, progress| -> CollectorFuture {
                *slot.lock().unwrap() = Some(progress);
                ranges.lock().unwrap().push(options);
                let release = release.clone();
                Box::pin(async move {
                    release.notified().await;
                    // The log of the range it was asked for.
                    let mut log = fixture_log();
                    log.meta.start = options.start.to_rfc3339();
                    log.meta.end = options.end.to_rfc3339();
                    Ok(log)
                })
            },
        );
        (held, collector)
    }

    /// Reports `log` the way the running collection would.
    async fn report(&self, log: ActivityLog) {
        for _ in 0..50 {
            if let Some(progress) = self.progress.lock().unwrap().clone() {
                return progress(log);
            }
            sleep(Duration::from_millis(10)).await;
        }
        panic!("the collector was never called");
    }
}

fn running_range(runtime: &CollectionRuntime) -> Option<(String, String)> {
    match runtime.activity_snapshot() {
        ActivitySnapshot::Running(log) => Some((log.meta.start.clone(), log.meta.end.clone())),
        _ => None,
    }
}

fn range_of(options: CollectOptions) -> Option<(String, String)> {
    Some((options.start.to_rfc3339(), options.end.to_rfc3339()))
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
        runtime.activity_snapshot(),
        ActivitySnapshot::Empty
    ));
}

#[test]
fn a_started_collection_is_running_with_every_usable_connection_pending() {
    let runtime = CollectionRuntime::new(collector_ok(), fixture_providers());

    let (snapshot, task) = runtime.start_collection(fixture_options());

    assert!(task.is_some());
    let ActivitySnapshot::Running(log) = snapshot else {
        panic!("expected a running collection");
    };
    assert_eq!(
        log.meta.pending,
        [ConnectionId::new(Source::Git, "default")]
    );
    assert!(log.entries.is_empty());
    assert_eq!(running_range(&runtime), range_of(fixture_options()));
}

#[test]
fn a_second_start_of_the_same_range_returns_the_running_collection_without_a_task() {
    let runtime = CollectionRuntime::new(collector_ok(), fixture_providers());
    let (_, _task) = runtime.start_collection(fixture_options());

    let (snapshot, task) = runtime.start_collection(fixture_options());

    assert!(task.is_none());
    assert!(matches!(
        snapshot,
        ActivitySnapshot::Running(log) if log.meta.start == fixture_options().start.to_rfc3339()
    ));
    assert_eq!(running_range(&runtime), range_of(fixture_options()));
}

#[tokio::test]
async fn a_start_of_another_range_stops_the_running_collection_and_collects_that_one() {
    let (held, collector) = HeldCollector::new();
    let runtime = CollectionRuntime::new(collector, fixture_providers());
    let (_, first) = runtime.start_collection(fixture_options());
    let stopped = tokio::spawn(first.unwrap());
    held.report(partial_log()).await;
    let stale = held.progress.lock().unwrap().take().unwrap();

    let (snapshot, second) = runtime.start_collection(other_options());

    assert!(matches!(
        snapshot,
        ActivitySnapshot::Running(log) if log.meta.start == other_options().start.to_rfc3339()
    ));
    assert_eq!(running_range(&runtime), range_of(other_options()));
    // The stopped run ends without a result and its late progress is dropped.
    stopped.await.unwrap();
    stale(partial_log());
    assert!(matches!(
        runtime.activity_snapshot(),
        ActivitySnapshot::Running(log) if log.entries.is_empty()
    ));

    let second = tokio::spawn(second.expect("a task for the new range"));
    held.report(partial_log()).await;
    held.release.notify_one();
    second.await.unwrap();
    assert_eq!(
        *held.ranges.lock().unwrap(),
        [fixture_options(), other_options()],
        "the restarted run collects the range asked for last"
    );
    assert!(matches!(
        runtime.activity_snapshot(),
        ActivitySnapshot::Ready(log) if log.meta.start == other_options().start.to_rfc3339()
    ));
}

#[tokio::test]
async fn progress_shows_in_the_snapshot_until_the_collection_ends() {
    let (held, collector) = HeldCollector::new();
    let runtime = CollectionRuntime::new(collector, fixture_providers());
    let (_, task) = runtime.start_collection(fixture_options());
    let task = tokio::spawn(task.unwrap());

    held.report(partial_log()).await;

    let ActivitySnapshot::Running(log) = runtime.activity_snapshot() else {
        panic!("expected a running collection");
    };
    assert_eq!(log.entries.len(), 1);
    assert_eq!(
        log.meta.pending,
        [ConnectionId::new(Source::Gitlab, "default")]
    );

    held.release.notify_one();
    task.await.unwrap();
    assert!(matches!(
        runtime.activity_snapshot(),
        ActivitySnapshot::Ready(log) if log.entries.is_empty()
    ));
}

#[tokio::test]
async fn progress_of_an_earlier_run_does_not_reach_the_next_one() {
    let (held, collector) = HeldCollector::new();
    let runtime = CollectionRuntime::new(collector, fixture_providers());
    let (_, first) = runtime.start_collection(fixture_options());
    let first = tokio::spawn(first.unwrap());
    held.report(fixture_log()).await;
    let stale = held.progress.lock().unwrap().clone().unwrap();
    held.release.notify_one();
    first.await.unwrap();

    let (_, _second) = runtime.start_collection(other_options());
    stale(partial_log());

    assert!(matches!(
        runtime.activity_snapshot(),
        ActivitySnapshot::Running(log) if log.entries.is_empty()
    ));
}

#[tokio::test]
async fn spawn_initial_collection_populates_cache() {
    let runtime = CollectionRuntime::new(collector_ok(), fixture_providers());

    let task = runtime
        .initial_collection_task()
        .expect("initial collection task");
    tokio::spawn(task);

    for _ in 0..20 {
        if matches!(runtime.activity_snapshot(), ActivitySnapshot::Ready(_)) {
            return;
        }
        sleep(Duration::from_millis(10)).await;
    }

    panic!("expected initial collection to populate the cache");
}

#[test]
fn the_initial_collection_is_running_before_its_task_is_spawned() {
    let runtime = CollectionRuntime::new(collector_ok(), fixture_providers());

    let _task = runtime.initial_collection_task();

    let ActivitySnapshot::Running(log) = runtime.activity_snapshot() else {
        panic!("expected the initial collection to run");
    };
    let running = resolve_collect_options(&request(&log.meta.start, &log.meta.end)).unwrap();
    assert!(runtime.start_collection(running).1.is_none());
}

fn assert_failed(runtime: &CollectionRuntime, expected: &str) {
    match runtime.activity_snapshot() {
        ActivitySnapshot::Failed { options, message } => {
            assert_eq!(options, fixture_options());
            assert!(message.contains(expected), "{message}");
        }
        _ => panic!("expected the failure"),
    }
    assert!(!runtime.is_collecting());
}

#[tokio::test]
async fn a_failed_collection_keeps_its_range_and_message() {
    let collector: Collector =
        Arc::new(|_options, _carried, _scope, _progress| -> CollectorFuture {
            Box::pin(async { Err(anyhow::anyhow!("GitLab unreachable")) })
        });
    let runtime = CollectionRuntime::new(collector, fixture_providers());

    runtime.start_collection(fixture_options()).1.unwrap().await;

    assert_failed(&runtime, "GitLab unreachable");
}

#[tokio::test]
async fn a_panicking_collection_ends_failed_instead_of_running_forever() {
    let collector: Collector =
        Arc::new(|_options, _carried, _scope, _progress| -> CollectorFuture {
            Box::pin(async { panic!("collector blew up") })
        });
    let runtime = CollectionRuntime::new(collector, fixture_providers());

    runtime.start_collection(fixture_options()).1.unwrap().await;

    assert_failed(&runtime, "collector blew up");
}

#[tokio::test]
async fn a_new_runtime_takes_over_a_running_collection() {
    let (held, collector) = HeldCollector::new();
    let previous = CollectionRuntime::new(collector, fixture_providers());
    let (_, task) = previous.start_collection(fixture_options());
    let stopped = tokio::spawn(task.unwrap());
    held.report(partial_log()).await;
    let next = CollectionRuntime::new(collector_ok(), fixture_providers());

    let task = next.take_over(&previous);

    assert_eq!(running_range(&next), range_of(fixture_options()));
    assert!(matches!(
        next.activity_snapshot(),
        ActivitySnapshot::Running(log) if log.entries.is_empty()
    ));
    // The stopped run ends without a result, though its collector never finished.
    stopped.await.unwrap();
    assert!(matches!(
        previous.activity_snapshot(),
        ActivitySnapshot::Running(_)
    ));

    task.expect("the collection to continue").await;
    assert!(matches!(
        next.activity_snapshot(),
        ActivitySnapshot::Ready(_)
    ));
}

#[tokio::test]
async fn a_new_runtime_collects_a_finished_range_again() {
    let previous = CollectionRuntime::new(collector_ok(), fixture_providers());
    previous
        .start_collection(fixture_options())
        .1
        .unwrap()
        .await;
    let next = CollectionRuntime::new(collector_ok(), fixture_providers());

    let task = next.take_over(&previous);

    assert!(task.is_some());
    assert_eq!(running_range(&next), range_of(fixture_options()));
}

#[tokio::test]
async fn a_new_runtime_collects_a_failed_range_again() {
    let failing: Collector = Arc::new(|_options, _carried, _scope, _progress| -> CollectorFuture {
        Box::pin(async { Err(anyhow::anyhow!("GitLab unreachable")) })
    });
    let previous = CollectionRuntime::new(failing, fixture_providers());
    previous
        .start_collection(fixture_options())
        .1
        .unwrap()
        .await;
    let next = CollectionRuntime::new(collector_ok(), fixture_providers());

    let task = next.take_over(&previous);

    assert_eq!(running_range(&next), range_of(fixture_options()));
    task.expect("the range to be collected again").await;
    assert!(matches!(
        next.activity_snapshot(),
        ActivitySnapshot::Ready(_)
    ));
}

#[test]
fn a_new_runtime_starts_nothing_when_nothing_was_collected() {
    let previous = CollectionRuntime::new(collector_ok(), fixture_providers());
    let next = CollectionRuntime::new(collector_ok(), fixture_providers());

    assert!(next.take_over(&previous).is_none());
    assert!(matches!(next.activity_snapshot(), ActivitySnapshot::Empty));
}

/// A Calendar connection without its URL: its check fails at once, without I/O.
fn unconfigured_calendar() -> Connection {
    Connection::new(
        ConnectionId::new(Source::Calendar, "team"),
        "Team",
        ConnectionLocation::Local,
        ConnectionValues::default(),
    )
}

#[tokio::test]
async fn sources_are_checking_until_the_checks_task_runs() {
    let (runtime, checks) = CollectionRuntime::start(&[unconfigured_calendar()]);

    let [pending] = runtime.provider_info().try_into().unwrap();
    assert_eq!(pending.status, ProviderStatus::Checking);
    assert!(!pending.usable);

    checks.await;
    let [checked] = runtime.provider_info().try_into().unwrap();
    assert_eq!(checked.status, ProviderStatus::Unavailable);
    assert_eq!(checked.label, "Team");
}

#[tokio::test]
async fn a_collection_runs_the_checks_it_needs() {
    let (runtime, _checks) = CollectionRuntime::start(&[unconfigured_calendar()]);

    let (snapshot, task) = runtime.start_collection(fixture_options());
    assert!(matches!(
        snapshot,
        ActivitySnapshot::Running(log)
            if log.meta.pending == [ConnectionId::new(Source::Calendar, "team")]
    ));
    task.unwrap().await;

    assert!(matches!(
        runtime.activity_snapshot(),
        ActivitySnapshot::Ready(log) if log.entries.is_empty() && log.meta.pending.is_empty()
    ));
    let [checked] = runtime.provider_info().try_into().unwrap();
    assert_eq!(checked.status, ProviderStatus::Unavailable);
}

fn request(start: &str, end: &str) -> CollectRequest {
    CollectRequest {
        start: start.to_string(),
        end: end.to_string(),
    }
}

#[test]
fn resolve_collect_options_reads_instants_with_any_offset() {
    let options = resolve_collect_options(&request(
        "2026-03-01T00:00:00+01:00",
        "2026-03-02T00:00:00Z",
    ))
    .unwrap();

    assert_eq!(options.start.to_rfc3339(), "2026-02-28T23:00:00+00:00");
    assert_eq!(options.end.to_rfc3339(), "2026-03-02T00:00:00+00:00");
}

#[test]
fn resolve_collect_options_rejects_dates_and_empty_ranges() {
    assert_eq!(
        resolve_collect_options(&request("2026-03-01", "2026-03-02")),
        Err("start and end must be RFC 3339 timestamps")
    );
    assert_eq!(
        resolve_collect_options(&request("2026-03-02T00:00:00Z", "2026-03-02T00:00:00Z")),
        Err("end must be after start")
    );
}

#[test]
fn resolve_collect_options_caps_the_range_at_ten_years() {
    assert!(
        resolve_collect_options(&request("2016-01-01T00:00:00Z", "2026-01-01T00:00:00Z")).is_ok()
    );
    assert_eq!(
        resolve_collect_options(&request("1000-01-01T00:00:00Z", "9999-12-31T00:00:00Z")),
        Err("A range can span at most 10 years (3653 days)")
    );
}

/// A finished log with one commit of the fixture's Git connection.
fn log_with_commit() -> ActivityLog {
    let mut log = fixture_log();
    let mut commit = GitEntry::new(
        "repo".to_string(),
        "abc123".to_string(),
        "message".to_string(),
        "me@example.com".to_string(),
        "main".to_string(),
        "2026-03-10T10:00:00Z".to_string(),
    );
    commit.connection = "default".to_string();
    log.entries = vec![ActivityEntry::Git(commit)];
    log
}

fn collector_returning(log: fn() -> ActivityLog) -> Collector {
    Arc::new(
        move |_options, _carried, _scope, _progress| -> CollectorFuture {
            Box::pin(async move { Ok(log()) })
        },
    )
}

fn first_entries(snapshot: ActivitySnapshot) -> usize {
    match snapshot {
        ActivitySnapshot::Running(log) => log.entries.len(),
        _ => panic!("expected a running collection"),
    }
}

#[tokio::test]
async fn collecting_again_shows_the_previous_entries_until_their_connection_is_in() {
    let runtime = CollectionRuntime::new(collector_returning(log_with_commit), fixture_providers());
    runtime.start_collection(fixture_options()).1.unwrap().await;

    let (snapshot, _task) = runtime.start_collection(fixture_options());

    assert_eq!(first_entries(snapshot), 1);
}

#[tokio::test]
async fn a_new_runtime_shows_what_the_old_one_showed_for_connections_it_still_has() {
    let previous =
        CollectionRuntime::new(collector_returning(log_with_commit), fixture_providers());
    previous
        .start_collection(fixture_options())
        .1
        .unwrap()
        .await;
    let next = CollectionRuntime::new(collector_ok(), fixture_providers());
    let mut renamed = fixture_providers();
    renamed[0].connection = ConnectionId::new(Source::Git, "laptop");
    let after_rename = CollectionRuntime::new(collector_ok(), renamed);

    let _task = next.take_over(&previous);
    let _renamed_task = after_rename.take_over(&previous);

    assert_eq!(first_entries(next.activity_snapshot()), 1);
    // The renamed connection is another one: nothing shows under the old name.
    assert_eq!(first_entries(after_rename.activity_snapshot()), 0);
}

#[tokio::test]
async fn a_collection_carries_into_the_collector_what_was_on_screen() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let collector: Collector = {
        let seen = seen.clone();
        Arc::new(
            move |_options, carried: Carried, _scope, _progress| -> CollectorFuture {
                seen.lock().unwrap().push(carried.len());
                Box::pin(async { Ok(log_with_commit()) })
            },
        )
    };
    let runtime = CollectionRuntime::new(collector, fixture_providers());
    runtime.start_collection(fixture_options()).1.unwrap().await;
    runtime.start_collection(fixture_options()).1.unwrap().await;

    assert_eq!(*seen.lock().unwrap(), [0, 1]);
}

fn team() -> ConnectionId {
    ConnectionId::new(Source::Calendar, "team")
}

#[tokio::test]
async fn recheck_runs_a_connections_startup_check_again() {
    let (runtime, checks) = CollectionRuntime::start(&[unconfigured_calendar()]);
    checks.await;

    let task = runtime
        .recheck(&team())
        .expect("a connection of this runtime");
    let [checking] = runtime.provider_info().try_into().unwrap();
    assert_eq!(checking.status, ProviderStatus::Checking);

    task.await;
    let [checked] = runtime.provider_info().try_into().unwrap();
    assert_eq!(checked.status, ProviderStatus::Unavailable);
    assert!(
        runtime
            .recheck(&ConnectionId::new(Source::Calendar, "other"))
            .is_none()
    );
}

#[tokio::test]
async fn recheck_after_a_collection_collects_only_the_connections_kind_again() {
    let (runtime, checks) = CollectionRuntime::start(&[unconfigured_calendar()]);
    checks.await;
    runtime.start_collection(fixture_options()).1.unwrap().await;

    let task = runtime.recheck(&team()).unwrap();
    assert!(matches!(
        runtime.activity_snapshot(),
        ActivitySnapshot::Running(log) if log.meta.pending == [team()]
    ));

    task.await;
    assert!(matches!(
        runtime.activity_snapshot(),
        ActivitySnapshot::Ready(log) if log.meta.pending.is_empty()
    ));
}

#[test]
fn a_recheck_keeps_the_other_kinds_with_each_connection_the_log_counts() {
    let mut log = log_with_commit();
    let git = ConnectionId::new(Source::Git, "default");
    let quiet = ConnectionId::new(Source::System, "this-computer");
    log.summary.connections = [(git.clone(), 1), (team(), 0), (quiet.clone(), 0)]
        .map(|(connection, count)| ConnectionCount { connection, count })
        .to_vec();

    let kept = kept_except(&log, &Source::Calendar);

    let kept = kept
        .iter()
        .map(|(id, entries)| (id.clone(), entries.len()))
        .collect::<Vec<_>>();
    assert_eq!(kept, [(git, 1), (quiet, 0)]);
}

#[test]
fn a_connection_whose_collection_failed_is_degraded_with_the_error() {
    let runtime = CollectionRuntime::new(collector_ok(), fixture_providers());
    runtime.inner.failures.lock().unwrap().insert(
        ConnectionId::new(Source::Git, "default"),
        "repository vanished".to_string(),
    );

    let [git] = runtime.provider_info().try_into().unwrap();
    assert_eq!(git.status, ProviderStatus::Degraded);
    assert!(git.usable, "it is still collected next time");
    assert_eq!(
        git.reason.as_deref(),
        Some("Collection failed: repository vanished")
    );
}
