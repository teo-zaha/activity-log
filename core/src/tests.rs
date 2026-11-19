use std::{
    path::Path,
    process::Command,
    sync::{Arc, Mutex},
};

use chrono::{TimeZone, Utc};
use futures::{FutureExt, future::BoxFuture};
use tokio::sync::Notify;

use crate::{
    collect_activity_detailed_with_provider_runtimes, collect_progressively,
    model::{
        ActivityEntry, ActivityLog, CollectOptions, ConnectionCount, ConnectionId, Source,
        SupportedOs, SystemEntry, SystemEntryType,
    },
    provider::{
        ProviderRuntime,
        connection::{Connection, ConnectionLocation, ConnectionValues},
        inspect_provider_runtimes_with,
        provider_adapter::ProviderAdapter,
        provider_check_result::ProviderCheckResult,
        provider_context::ProviderContext,
    },
};

fn git(repo: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(repo)
        .env("GIT_AUTHOR_DATE", "2026-03-05T09:00:00+00:00")
        .env("GIT_COMMITTER_DATE", "2026-03-05T09:00:00+00:00")
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

fn repo(email: Option<&str>, commits: &[(&str, &str)]) -> tempfile::TempDir {
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
    let collected = collect_activity_detailed_with_provider_runtimes(options, &runtimes).await;

    let mut commits = collected
        .log
        .entries
        .iter()
        .map(|entry| match entry {
            ActivityEntry::Git(commit) => (commit.connection.as_str(), commit.message.as_str()),
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
    let collected = collect_activity_detailed_with_provider_runtimes(options, &runtimes).await;

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
/// once `release` is notified, and `broken` never.
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

    fn startup_check(
        &self,
        _current_os: SupportedOs,
        _context: ProviderContext,
    ) -> BoxFuture<'_, ProviderCheckResult> {
        Box::pin(async { ProviderCheckResult::available() })
    }

    fn collect(
        &self,
        _current_os: SupportedOs,
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
                _ => 5,
            };
            Ok(vec![ActivityEntry::System(SystemEntry {
                source: "system".to_string(),
                connection: String::new(),
                entry_type: SystemEntryType::Unlock,
                timestamp: format!("2026-03-0{day}T09:00:00+00:00"),
                label: name,
            })])
        })
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
    let at_once = collect_activity_detailed_with_provider_runtimes(march(), &runtimes).await;
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

    let collected = collect_activity_detailed_with_provider_runtimes(march(), &runtimes).await;

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
