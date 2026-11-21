use std::{
    path::Path,
    process::Command,
    sync::{Arc, Mutex},
};

use chrono::{TimeZone, Utc};
use futures::{FutureExt, future::BoxFuture};
use tokio::sync::Notify;

use crate::{
    collect_planned, collect_progressively, collect_runtimes,
    model::{
        ActivityEntry, ActivityLog, CollectOptions, ConnectionCount, ConnectionId, LogLevel,
        Source, SupportedOs, SystemEntry, SystemEntryType,
    },
    provider::{
        ProviderRuntime,
        connection::{Connection, ConnectionLocation, ConnectionValues},
        inspect_provider_runtimes_with,
        provider_adapter::ProviderAdapter,
        provider_check_result::ProviderCheckResult,
        provider_context::ProviderContext,
    },
    runtime::{Checked, CollectionKey, Outcome, Plan, RunReport, Slot, Stored},
};

pub(crate) fn git(repo: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(repo)
        .env("GIT_AUTHOR_DATE", "2026-03-05T09:00:00+00:00")
        .env("GIT_COMMITTER_DATE", "2026-03-05T09:00:00+00:00")
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

pub(crate) fn repo(email: Option<&str>, commits: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q", "-b", "main"]);
    git(dir.path(), &["config", "user.name", "Me"]);
    if let Some(email) = email {
        git(dir.path(), &["config", "user.email", email]);
    }
    for (author, subject) in commits {
        let author = format!("--author=Someone <{author}>");
        git(
            dir.path(),
            &["commit", "-q", "--allow-empty", &author, "-m", subject],
        );
    }
    dir
}

fn git_connection(name: &str, dir: &Path) -> Connection {
    let dirs = serde_json::to_string(&[dir.display().to_string()]).unwrap();
    Connection::new(
        ConnectionId::new(Source::Git, name),
        name,
        ConnectionLocation::Local,
        ConnectionValues::new([("GIT_DIRS".to_string(), Some(dirs))].into()),
    )
}

/// The second connection's repository has no email of its own; the first one's email matches
/// its commits, and every entry carries the connection it came from.
#[tokio::test]
async fn git_connections_share_emails_and_entries_name_their_connection() {
    let work = repo(Some("me@example.com"), &[("me@example.com", "Work commit")]);
    let side = repo(
        None,
        &[
            ("ME@example.com", "Side commit"),
            ("other@example.com", "Not mine"),
        ],
    );
    let runtimes = inspect_provider_runtimes_with(&[
        git_connection("default", work.path()),
        git_connection("side", side.path()),
    ])
    .await;
    assert!(runtimes.iter().all(|runtime| runtime.usable()));

    let options = CollectOptions {
        start: Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap(),
        end: Utc.with_ymd_and_hms(2026, 4, 1, 0, 0, 0).unwrap(),
    };
    let collected = collect_runtimes(options, &runtimes).await.project(options);

    let mut commits = collected
        .log
        .entries
        .iter()
        .map(|entry| match entry {
            ActivityEntry::Git(commit) => {
                (commit.connection.as_str().unwrap(), commit.message.as_str())
            }
            other => panic!("unexpected entry {other:?}"),
        })
        .collect::<Vec<_>>();
    commits.sort();
    assert_eq!(
        commits,
        [("default", "Work commit"), ("side", "Side commit")]
    );
    assert_eq!(
        collected
            .summary
            .entry_count_for(&ConnectionId::new(Source::Git, "side")),
        1
    );
}

/// A clone holds the same commits as the original; each commit is listed once.
#[tokio::test]
async fn a_commit_in_two_clones_is_listed_once() {
    let original = repo(Some("me@example.com"), &[("me@example.com", "Shared work")]);
    let clones = tempfile::tempdir().unwrap();
    git(
        clones.path(),
        &[
            "clone",
            "-q",
            &original.path().display().to_string(),
            "copy",
        ],
    );
    git(
        &clones.path().join("copy"),
        &["config", "user.email", "me@example.com"],
    );
    let runtimes = inspect_provider_runtimes_with(&[
        git_connection("original", original.path()),
        git_connection("clones", clones.path()),
    ])
    .await;

    let options = CollectOptions {
        start: Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap(),
        end: Utc.with_ymd_and_hms(2026, 4, 1, 0, 0, 0).unwrap(),
    };
    let collected = collect_runtimes(options, &runtimes).await.project(options);

    assert_eq!(
        collected.log.entries.len(),
        1,
        "{:?}",
        collected.log.entries
    );
}

fn march() -> CollectOptions {
    CollectOptions {
        start: Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap(),
        end: Utc.with_ymd_and_hms(2026, 4, 1, 0, 0, 0).unwrap(),
    }
}

/// A source whose connections each collect one entry labelled with their name: `slow` only
/// once `release` is notified, and `broken` never; `off` fails its startup check and `twice`
/// collects its entry twice, as a source that repeats itself would. `noted` annotates its
/// entries (and logs that it does), `dawdles` never ends annotating them.
struct FakeSource {
    release: Arc<Notify>,
}

impl ProviderAdapter for FakeSource {
    fn id(&self) -> Source {
        Source::System
    }

    fn description(&self) -> &'static str {
        "A source for tests"
    }

    fn supported_os(&self) -> &'static [SupportedOs] {
        &[SupportedOs::Windows, SupportedOs::Macos, SupportedOs::Linux]
    }

    fn startup_check(&self, context: ProviderContext) -> BoxFuture<'_, ProviderCheckResult> {
        let off = context
            .connection()
            .is_some_and(|connection| connection.id.name == "off");
        Box::pin(async move {
            if off {
                ProviderCheckResult::unavailable("switched off")
            } else {
                ProviderCheckResult::available()
            }
        })
    }

    fn collect(
        &self,
        _options: CollectOptions,
        context: ProviderContext,
    ) -> BoxFuture<'_, anyhow::Result<Vec<ActivityEntry>>> {
        let name = context.connection().unwrap().id.name.clone();
        Box::pin(async move {
            let day = match name.as_str() {
                "slow" => {
                    self.release.notified().await;
                    6
                }
                "broken" => anyhow::bail!("unreachable host"),
                "stuck" => std::future::pending().await,
                _ => 5,
            };
            let copies = if name == "twice" { 2 } else { 1 };
            let entry = ActivityEntry::System(SystemEntry {
                source: crate::model::SystemTag,
                connection: crate::model::ConnectionName::unstamped(),
                id: name.clone(),
                entry_type: SystemEntryType::Unlock,
                timestamp: format!("2026-03-0{day}T09:00:00+00:00").into(),
                label: name,
            });
            Ok(vec![entry; copies])
        })
    }

    fn annotate<'a>(
        &'a self,
        entries: &'a mut [ActivityEntry],
        context: ProviderContext,
    ) -> BoxFuture<'a, ()> {
        let name = context.connection().unwrap().id.name.clone();
        Box::pin(async move {
            match name.as_str() {
                "noted" => {
                    // A warning, so the test tells it from the collection's line by its level.
                    crate::diagnostics::warn("system", "Annotating");
                    for entry in entries.iter_mut() {
                        if let ActivityEntry::System(entry) = entry {
                            entry.label = Self::annotated(&entry.label);
                        }
                    }
                }
                "dawdles" => std::future::pending().await,
                _ => {}
            }
        })
    }
}

impl FakeSource {
    fn annotated(label: &str) -> String {
        format!("{label}, annotated")
    }
}

fn fake_id(name: &str) -> ConnectionId {
    ConnectionId::new(Source::System, name)
}

async fn fake_runtimes(release: &Arc<Notify>, names: &[&str]) -> Vec<ProviderRuntime> {
    let adapter: Arc<dyn ProviderAdapter> = Arc::new(FakeSource {
        release: release.clone(),
    });
    let mut runtimes = Vec::new();
    for name in names {
        let connection = Connection::new(
            fake_id(name),
            *name,
            ConnectionLocation::Local,
            ConnectionValues::default(),
        );
        runtimes.push(ProviderRuntime::inspect(adapter.clone(), &connection).await);
    }
    runtimes
}

fn labels(log: &ActivityLog) -> Vec<&str> {
    log.entries
        .iter()
        .map(|entry| match entry {
            ActivityEntry::System(entry) => entry.label.as_str(),
            other => panic!("unexpected entry {other:?}"),
        })
        .collect()
}

fn counts(log: &ActivityLog) -> Vec<(&str, usize)> {
    log.summary
        .connections
        .iter()
        .map(|ConnectionCount { connection, count }| (connection.name.as_str(), *count))
        .collect()
}

/// The log as it goes over the wire, without the instant it was built.
fn wire(log: &ActivityLog) -> serde_json::Value {
    let mut value = serde_json::to_value(log).unwrap();
    value["meta"]["generated"] = serde_json::Value::Null;
    value
}

#[tokio::test]
async fn a_fast_connection_is_reported_while_a_slow_one_still_collects() {
    let release = Arc::new(Notify::new());
    let runtimes = fake_runtimes(&release, &["slow", "fast"]).await;
    let reported = Mutex::new(Vec::new());

    let connections = runtimes
        .iter()
        .map(|runtime| (runtime.id().clone(), std::future::ready(runtime.clone())))
        .collect();
    let collected = collect_progressively(march(), connections, |log| {
        reported.lock().unwrap().push(log);
        release.notify_one();
    })
    .await;

    let reported = reported.into_inner().unwrap();
    let [partial] = reported.as_slice() else {
        panic!("expected one partial log, got {}", reported.len());
    };
    assert_eq!(labels(partial), ["fast"]);
    assert_eq!(partial.meta.pending, [fake_id("slow")]);
    assert_eq!(counts(partial), [("fast", 1)], "pending is not listed yet");
    assert_eq!(wire(partial)["meta"]["pending"][0]["name"], "slow");

    assert_eq!(labels(&collected.log), ["slow", "fast"]);
    assert_eq!(counts(&collected.log), [("slow", 1), ("fast", 1)]);
    assert!(collected.log.meta.pending.is_empty());
    assert!(
        wire(&collected.log)["meta"].get("pending").is_none(),
        "a finished log has no pending field"
    );

    // The order the connections finish in does not change the result.
    release.notify_one();
    let at_once = collect_runtimes(march(), &runtimes).await.project(march());
    assert_eq!(wire(&at_once.log), wire(&collected.log));
}

#[tokio::test]
async fn a_blocked_startup_check_does_not_delay_another_connection() {
    let release = Arc::new(Notify::new());
    let runtimes = fake_runtimes(&release, &["checked-late", "fast"]).await;
    let checked = Arc::new(Notify::new());
    let reported = Mutex::new(Vec::new());

    let [late, fast] = <[ProviderRuntime; 2]>::try_from(runtimes).ok().unwrap();
    let late_check = {
        let checked = checked.clone();
        async move {
            checked.notified().await;
            late
        }
    };
    let connections = vec![
        (fake_id("checked-late"), late_check.boxed()),
        (fake_id("fast"), std::future::ready(fast).boxed()),
    ];
    let collected = collect_progressively(march(), connections, |log| {
        reported.lock().unwrap().push(log);
        checked.notify_one();
    })
    .await;

    let reported = reported.into_inner().unwrap();
    assert_eq!(labels(&reported[0]), ["fast"]);
    assert_eq!(reported[0].meta.pending, [fake_id("checked-late")]);
    assert_eq!(counts(&collected.log), [("checked-late", 1), ("fast", 1)]);
}

#[tokio::test]
async fn a_failed_connection_is_listed_without_entries() {
    let runtimes = fake_runtimes(&Arc::new(Notify::new()), &["broken", "fast"]).await;

    let collected = collect_runtimes(march(), &runtimes).await.project(march());

    assert_eq!(labels(&collected.log), ["fast"]);
    assert_eq!(counts(&collected.log), [("broken", 0), ("fast", 1)]);
}

/// While the connection that knows the email is still being checked, the other's commits
/// cannot be told apart yet: they show up once it is in.
#[tokio::test]
async fn git_commits_appear_once_their_email_is_found() {
    let side = repo(None, &[("me@example.com", "Side commit")]);
    let work = repo(Some("me@example.com"), &[("me@example.com", "Work commit")]);
    let runtimes = inspect_provider_runtimes_with(&[
        git_connection("side", side.path()),
        git_connection("work", work.path()),
    ])
    .await;
    let checked = Arc::new(Notify::new());
    let reported = Mutex::new(Vec::new());

    let [side, work] = <[ProviderRuntime; 2]>::try_from(runtimes).ok().unwrap();
    let work_check = {
        let checked = checked.clone();
        async move {
            checked.notified().await;
            work
        }
    };
    let connections = vec![
        (side.id().clone(), std::future::ready(side).boxed()),
        (ConnectionId::new(Source::Git, "work"), work_check.boxed()),
    ];
    let collected = collect_progressively(march(), connections, |log| {
        reported.lock().unwrap().push(log);
        checked.notify_one();
    })
    .await;

    let reported = reported.into_inner().unwrap();
    assert!(reported[0].entries.is_empty(), "{:?}", reported[0].entries);
    assert_eq!(reported[0].meta.pending.len(), 1);
    assert_eq!(collected.log.entries.len(), 2);
}

fn carried_entry(name: &str) -> ActivityEntry {
    ActivityEntry::System(SystemEntry {
        source: crate::model::SystemTag,
        connection: name.into(),
        id: format!("old {name}"),
        entry_type: SystemEntryType::Lock,
        timestamp: "2026-03-02T09:00:00+00:00".into(),
        label: format!("old {name}"),
    })
}

fn stored(entries: Vec<ActivityEntry>) -> Stored {
    Stored {
        key: CollectionKey::UNKEYED,
        entries: entries.into(),
    }
}

fn collect_plan(runtime: &ProviderRuntime, carried: Option<Stored>) -> (ConnectionId, Plan) {
    let check = std::future::ready(Ok(Checked {
        runtime: runtime.clone(),
        key: CollectionKey::UNKEYED,
    }))
    .boxed();
    (runtime.id().clone(), Plan::Collect { check, carried })
}

#[tokio::test]
async fn pending_connections_show_their_carried_entries_until_they_are_in() {
    let release = Arc::new(Notify::new());
    let runtimes = fake_runtimes(&release, &["slow", "broken", "off", "fast"]).await;
    assert!(!runtimes[2].usable());
    let plans = runtimes
        .iter()
        .map(|runtime| {
            let name = runtime.id().name.clone();
            collect_plan(runtime, Some(stored(vec![carried_entry(&name)])))
        })
        .collect();
    let reported = Mutex::new(Vec::new());

    let report = collect_planned(march(), plans, |report: RunReport| {
        reported.lock().unwrap().push(report.project(march()).log);
        release.notify_one();
    })
    .await;

    let reported = reported.into_inner().unwrap();
    assert!(!reported.is_empty());
    for partial in &reported {
        for label in labels(partial) {
            if let Some(name) = label.strip_prefix("old ") {
                assert!(
                    partial.meta.pending.contains(&fake_id(name)),
                    "{label} is shown though its connection settled"
                );
            }
        }
        assert!(
            counts(partial)
                .iter()
                .all(|(name, _)| !partial.meta.pending.contains(&fake_id(name)))
        );
    }
    assert!(
        reported
            .iter()
            .any(|partial| labels(partial).contains(&"old slow")),
        "the slow connection shows its carried entry while it collects"
    );
    assert_eq!(labels(&report.project(march()).log), ["slow", "fast"]);
}

#[tokio::test]
async fn entries_a_source_repeats_get_ids_of_their_own_and_one_warning() {
    let release = Arc::new(Notify::new());
    let runtimes = fake_runtimes(&release, &["twice", "fast"]).await;

    let collected = collect_runtimes(march(), &runtimes).await.project(march());

    let mut ids = collected
        .log
        .entries
        .iter()
        .map(|entry| {
            (
                entry.connection().unwrap().to_string(),
                entry.id().to_string(),
            )
        })
        .collect::<Vec<_>>();
    ids.sort();
    assert_eq!(
        ids,
        [
            ("fast".to_string(), "fast".to_string()),
            ("twice".to_string(), "twice".to_string()),
            ("twice".to_string(), "twice#2".to_string()),
        ]
    );
    let warnings = crate::diagnostics::entries()
        .into_iter()
        .filter(|entry| entry.scope == "system/twice" && entry.message.contains("Numbered"))
        .count();
    assert_eq!(warnings, 1);
}

/// Two clones of one repository in one connection: Git lists the commit once, so its id is not
/// numbered and nothing is reported.
#[tokio::test]
async fn a_commit_two_clones_of_one_connection_hold_is_not_numbered() {
    let original = repo(Some("me@example.com"), &[("me@example.com", "Shared work")]);
    let parent = tempfile::tempdir().unwrap();
    for name in ["one", "two"] {
        git(
            parent.path(),
            &["clone", "-q", &original.path().display().to_string(), name],
        );
        git(
            &parent.path().join(name),
            &["config", "user.email", "me@example.com"],
        );
    }
    let runtimes =
        inspect_provider_runtimes_with(&[git_connection("both-clones", parent.path())]).await;

    let collected = collect_runtimes(march(), &runtimes).await.project(march());

    let ids = collected
        .log
        .entries
        .iter()
        .map(|entry| entry.id().to_string())
        .collect::<Vec<_>>();
    assert_eq!(ids.len(), 1, "{ids:?}");
    assert!(!ids[0].contains('#'));
    assert!(
        !crate::diagnostics::entries()
            .iter()
            .any(|entry| entry.scope == "git/both-clones" && entry.message.contains("Numbered"))
    );
}

/// A collection that does not end in time is a timeout, not any failure.
#[tokio::test(start_paused = true)]
async fn a_collection_that_never_ends_times_out() {
    let release = Arc::new(Notify::new());
    let runtimes = fake_runtimes(&release, &["stuck"]).await;
    let plans = runtimes
        .iter()
        .map(|runtime| collect_plan(runtime, None))
        .collect();

    let report = collect_planned(march(), plans, |_| {}).await;

    match &report.slots[0].1 {
        Slot::Settled(Outcome::Failed { failure, .. }) => {
            assert_eq!(failure.problem(), crate::model::ProviderProblem::TimedOut);
        }
        other => panic!("expected a timeout, got {other:?}"),
    }
}

#[tokio::test]
async fn a_planned_collection_keeps_what_it_does_not_collect_and_reports_what_failed() {
    let release = Arc::new(Notify::new());
    let runtimes = fake_runtimes(&release, &["fast", "broken"]).await;
    let kept = Outcome::Collected(stored(vec![carried_entry("kept")]));
    let mut plans = vec![(fake_id("kept"), Plan::Keep(kept))];
    plans.extend(runtimes.iter().map(|runtime| collect_plan(runtime, None)));
    // Counted without entries, as the log it is kept from counted it.
    plans.push((
        fake_id("quiet"),
        Plan::Keep(Outcome::Collected(stored(Vec::new()))),
    ));

    let report = collect_planned(march(), plans, |_| {}).await;

    let log = report.project(march()).log;
    assert_eq!(
        counts(&log),
        [("kept", 1), ("fast", 1), ("broken", 0), ("quiet", 0)]
    );
    assert!(log.meta.pending.is_empty());
    let failed = report
        .slots
        .iter()
        .filter_map(|(id, slot)| match slot {
            Slot::Settled(Outcome::Failed { failure, .. }) => {
                Some((id.clone(), failure.message().to_string()))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        failed,
        [(fake_id("broken"), "unreachable host".to_string())]
    );
    assert!(matches!(report.slots[0].1, Slot::Kept(_)));
}

/// What a kind annotates reaches the report, after the collection's own time is logged.
#[tokio::test]
async fn annotations_follow_the_logged_collection() {
    let release = Arc::new(Notify::new());
    let runtimes = fake_runtimes(&release, &["noted", "fast"]).await;

    let collected = collect_runtimes(march(), &runtimes).await.project(march());

    let mut labels = labels(&collected.log);
    labels.sort();
    assert_eq!(labels, ["fast", FakeSource::annotated("noted").as_str()]);
    // The check's line, the collection's, then the annotation's warning.
    let levels: Vec<LogLevel> = crate::diagnostics::entries()
        .into_iter()
        .filter(|line| line.scope == "system/noted")
        .map(|line| line.level)
        .collect();
    assert_eq!(levels, [LogLevel::Info, LogLevel::Info, LogLevel::Warn]);
}

/// An annotation that never ends is stopped; the connection keeps its entries and is not
/// failed.
#[tokio::test(start_paused = true)]
async fn an_annotation_that_never_ends_costs_no_entries() {
    let release = Arc::new(Notify::new());
    let runtimes = fake_runtimes(&release, &["dawdles"]).await;
    let plans = runtimes
        .iter()
        .map(|runtime| collect_plan(runtime, None))
        .collect();
    let started = tokio::time::Instant::now();

    // Bounded here too, so a missing safety timeout fails the test instead of hanging it.
    let report = tokio::time::timeout(
        crate::ANNOTATE_TIMEOUT * 2,
        collect_planned(march(), plans, |_| {}),
    )
    .await
    .expect("the safety timeout stops the annotation");

    match &report.slots[0].1 {
        Slot::Settled(Outcome::Collected(stored)) => {
            assert_eq!(stored.entries.len(), 1);
        }
        other => panic!("expected the entries, got {other:?}"),
    }
    assert_eq!(started.elapsed(), crate::ANNOTATE_TIMEOUT);
    assert!(crate::ANNOTATE_TIMEOUT > crate::collect::change_stack::STACK_BUDGET);
    assert!(
        crate::diagnostics::entries().iter().any(
            |line| line.scope == "system/dawdles" && line.level == crate::model::LogLevel::Warn
        )
    );
}
