use std::{
    cell::RefCell,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Barrier},
    time::Duration,
};

use chrono::{DateTime, NaiveDate, Utc};

use super::{CachePolicy, collect_with_day_cache_at, recent_ttl};
use crate::model::{
    ActivityEntry, CollectOptions, ConnectionId, Source, SystemEntry, SystemEntryType,
};

const POLICY: CachePolicy = CachePolicy {
    recent_ttl: Duration::from_secs(5 * 60),
    settled_ttl: Duration::from_secs(7 * 24 * 60 * 60),
};

fn day(d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, d).unwrap()
}

fn at(d: u32, hour: u32) -> DateTime<Utc> {
    day(d).and_hms_opt(hour, 0, 0).unwrap().and_utc()
}

/// Whole UTC days `since..=until`.
fn range(since: u32, until: u32) -> CollectOptions {
    CollectOptions {
        start: at(since, 0),
        end: at(until + 1, 0),
    }
}

fn entry(d: u32) -> ActivityEntry {
    ActivityEntry::System(SystemEntry {
        source: "system".to_string(),
        connection: String::new(),
        id: format!("day {d}"),
        entry_type: SystemEntryType::Boot,
        timestamp: at(d, 12).to_rfc3339(),
        label: format!("day {d}"),
    })
}

fn labels(entries: &[ActivityEntry]) -> Vec<String> {
    entries
        .iter()
        .map(|entry| match entry {
            ActivityEntry::System(entry) => entry.label.clone(),
            other => panic!("unexpected entry {other:?}"),
        })
        .collect()
}

/// Runs a cached collect whose fetch returns one entry at noon per day it was asked for and
/// records those days, first and last inclusive.
async fn collect(
    path: &Path,
    options: CollectOptions,
    now: DateTime<Utc>,
    calls: &RefCell<Vec<(NaiveDate, NaiveDate)>>,
) -> Vec<ActivityEntry> {
    collect_with_day_cache_at(
        path,
        &ConnectionId::new(Source::Jira, "default"),
        &options,
        POLICY,
        move || now,
        |span| async move {
            let (first, last) = span.utc_days();
            calls.borrow_mut().push((first, last));
            let entries = super::days(first, last)
                .map(|d| entry(d.format("%d").to_string().parse().unwrap()))
                .collect();
            Ok(entries)
        },
    )
    .await
    .unwrap()
}

fn cache_path() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("jira-test.json");
    (dir, path)
}

#[tokio::test]
async fn serves_a_repeated_range_from_disk_without_fetching() {
    let (_dir, path) = cache_path();
    let calls = RefCell::new(Vec::new());

    let first = collect(&path, range(10, 12), at(20, 9), &calls).await;
    let second = collect(&path, range(10, 12), at(20, 9), &calls).await;

    assert_eq!(labels(&first), ["day 10", "day 11", "day 12"]);
    assert_eq!(labels(&second), labels(&first));
    assert_eq!(calls.borrow().len(), 1, "second collect must not fetch");
}

#[tokio::test]
async fn fetches_only_the_missing_days() {
    let (_dir, path) = cache_path();
    let calls = RefCell::new(Vec::new());

    collect(&path, range(10, 12), at(20, 9), &calls).await;
    let widened = collect(&path, range(10, 15), at(20, 9), &calls).await;

    assert_eq!(
        labels(&widened),
        ["day 10", "day 11", "day 12", "day 13", "day 14", "day 15"]
    );
    assert_eq!(calls.borrow()[1], (day(13), day(15)));
}

#[tokio::test]
async fn refreshes_recent_days_but_keeps_settled_ones() {
    let (_dir, path) = cache_path();
    let calls = RefCell::new(Vec::new());

    // Fetched on the 20th at 09:00: days 10..19 are settled, the 20th is still running.
    collect(&path, range(10, 20), at(20, 9), &calls).await;
    collect(&path, range(10, 20), at(20, 10), &calls).await;

    assert_eq!(calls.borrow()[1], (day(20), day(20)));
}

#[tokio::test]
async fn a_day_fetched_before_it_ended_is_not_treated_as_settled() {
    let (_dir, path) = cache_path();
    let calls = RefCell::new(Vec::new());

    collect(&path, range(20, 20), at(20, 9), &calls).await;
    // A week later the 20th is long over, but the cached copy only covers its morning.
    collect(&path, range(20, 20), at(27, 9), &calls).await;

    assert_eq!(calls.borrow().len(), 2);
}

#[tokio::test]
async fn cuts_cached_days_to_the_requested_instants() {
    let (_dir, path) = cache_path();
    let calls = RefCell::new(Vec::new());

    // From 13:00 on the 10th to noon on the 12th: the noon entries of the 10th (too early) and
    // the 12th (the end is exclusive) fall outside, although both days are fetched.
    let options = CollectOptions {
        start: at(10, 13),
        end: at(12, 12),
    };
    let entries = collect(&path, options, at(20, 9), &calls).await;

    assert_eq!(labels(&entries), ["day 11"]);
    assert_eq!(calls.borrow()[0], (day(10), day(12)));
}

#[tokio::test]
async fn ignores_a_corrupt_cache_file() {
    let (_dir, path) = cache_path();
    fs::write(&path, "{not json").unwrap();
    let calls = RefCell::new(Vec::new());

    let entries = collect(&path, range(10, 10), at(20, 9), &calls).await;

    assert_eq!(labels(&entries), ["day 10"]);
}

#[tokio::test]
async fn a_failed_cache_write_does_not_fail_the_collection() {
    let dir = tempfile::tempdir().unwrap();
    // The parent "directory" is a file, so creating the cache dir fails.
    let blocker = dir.path().join("blocker");
    fs::write(&blocker, "").unwrap();
    let path = blocker.join("jira.json");
    let calls = RefCell::new(Vec::new());

    let entries = collect(&path, range(10, 10), at(20, 9), &calls).await;

    assert_eq!(labels(&entries), ["day 10"]);
}

#[tokio::test]
async fn propagates_fetch_errors() {
    let (_dir, path) = cache_path();

    let result = collect_with_day_cache_at(
        &path,
        &ConnectionId::new(Source::Jira, "default"),
        &range(10, 10),
        POLICY,
        || at(20, 9),
        |_| async { Err(anyhow::anyhow!("offline")) },
    )
    .await;

    assert_eq!(result.unwrap_err().to_string(), "offline");
}

/// Files written before entries had ids and GitLab pushes named their commit (version 3) are
/// read again from the API, without a warning that the file is unreadable.
#[tokio::test]
async fn refetches_days_from_a_file_of_an_older_version() {
    let (_dir, path) = cache_path();
    let calls = RefCell::new(Vec::new());
    collect(&path, range(10, 10), at(20, 9), &calls).await;
    let current: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    let mut older = current.clone();
    older["version"] = serde_json::json!(3);
    // Version 3 entries had no `id`.
    for day in older["days"].as_object_mut().unwrap().values_mut() {
        for entry in day["entries"].as_array_mut().unwrap() {
            entry["entry"].as_object_mut().unwrap().remove("id");
        }
    }
    assert_ne!(older, current);
    fs::write(&path, older.to_string()).unwrap();

    let entries = collect(&path, range(10, 10), at(20, 9), &calls).await;

    assert_eq!(labels(&entries), ["day 10"]);
    assert_eq!(calls.borrow().len(), 2, "the older file must not be served");
    let rewritten: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(rewritten["version"], serde_json::json!(super::FILE_VERSION));
    let scope = ConnectionId::new(Source::Jira, "default").to_string();
    assert!(
        !crate::diagnostics::entries().iter().any(
            |entry| entry.scope == scope && entry.message.contains(&path.display().to_string())
        )
    );
}

/// B11: a value too large for a duration of minutes no longer overflows.
#[test]
fn the_recent_ttl_override_is_checked() {
    assert_eq!(recent_ttl(Some("30")), Duration::from_secs(30 * 60));
    assert_eq!(recent_ttl(Some(" 0 ")), Duration::ZERO);
    for fallback in [None, Some("soon"), Some("-1"), Some("18446744073709551615")] {
        assert_eq!(
            recent_ttl(fallback),
            Duration::from_secs(5 * 60),
            "{fallback:?}"
        );
    }
}

fn labelled(tag: &str, d: u32) -> ActivityEntry {
    let ActivityEntry::System(mut entry) = entry(d) else {
        unreachable!()
    };
    entry.label = format!("{tag} {d}");
    ActivityEntry::System(entry)
}

fn day_number(day: NaiveDate) -> u32 {
    day.format("%d").to_string().parse().unwrap()
}

/// A cached collect of `options` judged at `now`, whose fetch labels its entries with `tag`.
async fn collect_tagged(
    path: &Path,
    options: CollectOptions,
    now: DateTime<Utc>,
    tag: &str,
) -> anyhow::Result<Vec<ActivityEntry>> {
    collect_tagged_with(path, options, POLICY, now, tag).await
}

async fn collect_tagged_with(
    path: &Path,
    options: CollectOptions,
    policy: CachePolicy,
    now: DateTime<Utc>,
    tag: &str,
) -> anyhow::Result<Vec<ActivityEntry>> {
    collect_with_day_cache_at(
        path,
        &ConnectionId::new(Source::Jira, "default"),
        &options,
        policy,
        move || now,
        |span| async move {
            let (first, last) = span.utc_days();
            Ok(super::days(first, last)
                .map(|d| labelled(tag, day_number(d)))
                .collect())
        },
    )
    .await
}

/// What the file holds for `options`, read without fetching (a fetch would panic).
async fn stored(path: &Path, options: CollectOptions) -> Vec<String> {
    let entries = collect_with_day_cache_at(
        path,
        &ConnectionId::new(Source::Jira, "default"),
        &options,
        CachePolicy {
            recent_ttl: Duration::MAX,
            settled_ttl: Duration::MAX,
        },
        || at(28, 0),
        |_| async { panic!("a stored day was fetched") },
    )
    .await
    .unwrap();
    labels(&entries)
}

/// B8: a day refreshed meanwhile sits between stale ones. Only the runs around it are fetched,
/// and its newer copy is kept.
#[tokio::test]
async fn a_fresh_middle_day_is_neither_fetched_nor_rewritten() {
    let (_dir, path) = cache_path();
    let short = CachePolicy {
        recent_ttl: Duration::from_secs(5 * 60),
        settled_ttl: Duration::from_secs(5 * 60),
    };
    let first = at(20, 9);
    let refreshed = first + Duration::from_secs(10 * 60);
    let later = refreshed + Duration::from_secs(60);
    collect_tagged_with(&path, range(10, 14), short, first, "old")
        .await
        .unwrap();
    collect_tagged_with(&path, range(12, 12), short, refreshed, "updated")
        .await
        .unwrap();

    let calls = RefCell::new(Vec::new());
    let entries = collect_with_day_cache_at(
        &path,
        &ConnectionId::new(Source::Jira, "default"),
        &range(10, 14),
        short,
        move || later,
        |span| {
            let calls = &calls;
            async move {
                let (first, last) = span.utc_days();
                calls.borrow_mut().push((first, last));
                Ok(super::days(first, last)
                    .map(|d| labelled("new", day_number(d)))
                    .collect())
            }
        },
    )
    .await
    .unwrap();

    assert_eq!(*calls.borrow(), [(day(10), day(11)), (day(13), day(14))]);
    assert_eq!(
        labels(&entries),
        ["new 10", "new 11", "updated 12", "new 13", "new 14"]
    );
    assert_eq!(
        stored(&path, range(10, 14)).await,
        ["new 10", "new 11", "updated 12", "new 13", "new 14"]
    );
}

/// B7: a writer that fetched earlier and writes later keeps the other writer's days, and the
/// days both fetched stay as the later-started fetch saw them.
#[tokio::test]
async fn parallel_writers_keep_both_days_and_the_later_started_fetch_wins() {
    let (_dir, path) = cache_path();
    let b_done = tokio::sync::Notify::new();
    let early = at(20, 9);
    let late = early + Duration::from_secs(1);
    let (connection, a_range) = (ConnectionId::new(Source::Jira, "default"), range(10, 12));

    let a = collect_with_day_cache_at(
        &path,
        &connection,
        &a_range,
        POLICY,
        move || early,
        |span| {
            let b_done = &b_done;
            async move {
                // Started first, finishes after B has written.
                b_done.notified().await;
                let (first, last) = span.utc_days();
                Ok(super::days(first, last)
                    .map(|d| labelled("a", day_number(d)))
                    .collect())
            }
        },
    );
    let b = async {
        let entries = collect_tagged(&path, range(11, 13), late, "b").await;
        b_done.notify_one();
        entries
    };
    let (a, b) = tokio::join!(a, b);

    assert_eq!(labels(&a.unwrap()), ["a 10", "a 11", "a 12"]);
    assert_eq!(labels(&b.unwrap()), ["b 11", "b 12", "b 13"]);
    assert_eq!(
        stored(&path, range(10, 13)).await,
        ["a 10", "b 11", "b 12", "b 13"]
    );
}

/// B7: writers in parallel threads (one runtime each, as separate processes would be) each
/// add a day; none is lost to another's rename.
#[test]
fn writers_at_the_same_time_lose_no_day() {
    let (_dir, path) = cache_path();
    let barrier = Arc::new(Barrier::new(6));
    let writers: Vec<_> = (10..16)
        .map(|d| {
            let (path, barrier) = (path.clone(), barrier.clone());
            std::thread::spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();
                barrier.wait();
                runtime
                    .block_on(collect_tagged(&path, range(d, d), at(20, 9), "w"))
                    .unwrap();
            })
        })
        .collect();
    for writer in writers {
        writer.join().unwrap();
    }

    let runtime = tokio::runtime::Runtime::new().unwrap();
    assert_eq!(
        runtime.block_on(stored(&path, range(10, 15))),
        ["w 10", "w 11", "w 12", "w 13", "w 14", "w 15"]
    );
}

/// A later run of days that fails fails the collection, but the run before it is kept.
#[tokio::test]
async fn a_failed_later_run_still_writes_the_earlier_one() {
    let (_dir, path) = cache_path();
    // Day 12 cached and fresh: days 10..14 are fetched as 10..11, then 13..14.
    collect_tagged(&path, range(12, 12), at(20, 9), "cached")
        .await
        .unwrap();

    let result = collect_with_day_cache_at(
        &path,
        &ConnectionId::new(Source::Jira, "default"),
        &range(10, 14),
        POLICY,
        || at(20, 9),
        |span| async move {
            let (first, last) = span.utc_days();
            if first == day(13) {
                anyhow::bail!("offline");
            }
            Ok(super::days(first, last)
                .map(|d| labelled("first run", day_number(d)))
                .collect())
        },
    )
    .await;

    assert_eq!(result.unwrap_err().to_string(), "offline");
    assert_eq!(
        stored(&path, range(10, 12)).await,
        ["first run 10", "first run 11", "cached 12"]
    );
}

/// A writer whose later run is cut off (the connection's deadline drops the collection) has
/// already written the runs it completed.
#[tokio::test]
async fn completed_runs_are_kept_when_a_later_run_is_cut_off() {
    let (_dir, path) = cache_path();
    collect_tagged(&path, range(12, 12), at(20, 9), "cached")
        .await
        .unwrap();

    let cut_off = tokio::time::timeout(
        Duration::from_millis(200),
        collect_with_day_cache_at(
            &path,
            &ConnectionId::new(Source::Jira, "default"),
            &range(10, 14),
            POLICY,
            || at(20, 9),
            |span| async move {
                let (first, last) = span.utc_days();
                if first == day(13) {
                    std::future::pending::<()>().await;
                }
                Ok(super::days(first, last)
                    .map(|d| labelled("first run", day_number(d)))
                    .collect())
            },
        ),
    )
    .await;

    assert!(cut_off.is_err(), "the second run never ends");
    assert_eq!(
        stored(&path, range(10, 12)).await,
        ["first run 10", "first run 11", "cached 12"]
    );
}

/// Each run's days carry the instant that run's fetch started, not the collection's start:
/// a writer that started between A's two runs wins A's first run's overlapping day and loses
/// A's second run's.
#[tokio::test]
async fn each_run_is_dated_by_the_start_of_its_own_fetch() {
    let (_dir, path) = cache_path();
    let t0 = at(20, 9);
    let (t1, t2, t3) = (
        t0 + Duration::from_secs(1),
        t0 + Duration::from_secs(2),
        t0 + Duration::from_secs(3),
    );
    collect_tagged(&path, range(12, 12), t0, "cached")
        .await
        .unwrap();
    // A's clock: the collection's start, then the start of each run.
    let a_clock = std::sync::Mutex::new(vec![t3, t1, t0]);
    let zero_ttl = CachePolicy {
        recent_ttl: Duration::ZERO,
        settled_ttl: Duration::ZERO,
    };

    let path_ref = &path;
    let a = collect_with_day_cache_at(
        &path,
        &ConnectionId::new(Source::Jira, "default"),
        &range(10, 14),
        POLICY,
        || a_clock.lock().unwrap().pop().unwrap(),
        |span| async move {
            let (first, last) = span.utc_days();
            if first == day(13) {
                // B starts at t2 and writes days 11 to 13 before A's second run returns.
                collect_tagged_with(path_ref, range(11, 13), zero_ttl, t2, "b")
                    .await
                    .unwrap();
            }
            Ok(super::days(first, last)
                .map(|d| labelled("a", day_number(d)))
                .collect())
        },
    )
    .await
    .unwrap();

    assert_eq!(labels(&a), ["a 10", "a 11", "cached 12", "a 13", "a 14"]);
    assert_eq!(
        stored(&path, range(10, 14)).await,
        ["a 10", "b 11", "b 12", "a 13", "a 14"]
    );
}

/// A day fetched at the same instant as the stored copy keeps the stored copy.
#[tokio::test]
async fn a_tie_keeps_the_stored_day() {
    let (_dir, path) = cache_path();
    let path_ref = &path;
    let a = collect_with_day_cache_at(
        &path,
        &ConnectionId::new(Source::Jira, "default"),
        &range(10, 10),
        POLICY,
        || at(20, 9),
        |span| async move {
            // B, started at the same instant, stores day 10 first.
            collect_tagged(path_ref, range(10, 10), at(20, 9), "b")
                .await
                .unwrap();
            let (first, last) = span.utc_days();
            Ok(super::days(first, last)
                .map(|d| labelled("a", day_number(d)))
                .collect())
        },
    )
    .await
    .unwrap();

    assert_eq!(labels(&a), ["a 10"]);
    assert_eq!(stored(&path, range(10, 10)).await, ["b 10"]);
}

/// A stored day dated where no fetch can have started (out of range, or after now) is never
/// fresh; the next fetch of it replaces it.
#[tokio::test]
async fn an_impossible_stored_date_is_replaced() {
    let year_2100 = chrono::NaiveDate::from_ymd_opt(2100, 1, 1)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc()
        .timestamp_millis();
    for impossible in [i64::MAX, year_2100] {
        let (_dir, path) = cache_path();
        collect_tagged(&path, range(10, 10), at(20, 9), "old")
            .await
            .unwrap();
        let mut file: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        file["days"]["2026-10-10"]["fetched_at"] = serde_json::json!(impossible);
        fs::write(&path, file.to_string()).unwrap();

        let fetched = collect_tagged(&path, range(10, 10), at(20, 9), "new")
            .await
            .unwrap();

        assert_eq!(labels(&fetched), ["new 10"], "{impossible}");
        assert_eq!(
            stored(&path, range(10, 10)).await,
            ["new 10"],
            "{impossible}"
        );
    }
}

/// Serde's message for a bad file can quote the file; the warning gives only where it broke.
#[tokio::test]
async fn an_unreadable_file_is_reported_without_its_content() {
    let (_dir, path) = cache_path();
    fs::write(
        &path,
        format!(
            r#"{{"version":{},"days":{{"2026-10-10":{{"fetched_at":1,"entries":[{{"kind":"private meeting title","entry":{{}}}}]}}}}}}"#,
            super::FILE_VERSION
        ),
    )
    .unwrap();

    collect_tagged(&path, range(10, 10), at(20, 9), "x")
        .await
        .unwrap();

    let warnings: Vec<String> = crate::diagnostics::entries()
        .into_iter()
        .map(|entry| entry.message)
        .filter(|message| message.contains(&path.display().to_string()))
        .collect();
    assert!(!warnings.is_empty());
    assert!(
        warnings
            .iter()
            .all(|message| !message.contains("private meeting title")),
        "{warnings:?}"
    );
}

/// Under the lock, only a missing file or one of an older version is replaced. A file that
/// does not decode, or one of a newer version, may hold days this writer cannot see: it is
/// left as it is.
#[tokio::test]
async fn a_file_that_does_not_decode_under_the_lock_is_left_as_it_is() {
    let undecodable = format!(
        r#"{{"version":{},"days":{{"2026-10-20":{{"fetched_at":1,"entries":[{{"kind":"bogus","entry":{{}}}}]}}}}}}"#,
        super::FILE_VERSION
    );
    let newer = format!(
        r#"{{"version":{},"days":{{"2026-10-20":{{"fetched_at":1,"layout":"new"}}}}}}"#,
        super::FILE_VERSION + 1
    );
    for found in [undecodable, newer, "{not json".to_string()] {
        let (_dir, path) = cache_path();
        let entries = collect_with_day_cache_at(
            &path,
            &ConnectionId::new(Source::Jira, "default"),
            &range(10, 10),
            POLICY,
            || at(20, 9),
            |span| {
                let (path, found) = (&path, &found);
                async move {
                    // Another writer leaves this file after the first read.
                    fs::write(path, found).unwrap();
                    let (first, last) = span.utc_days();
                    Ok(super::days(first, last)
                        .map(|d| labelled("mine", day_number(d)))
                        .collect())
                }
            },
        )
        .await
        .unwrap();

        assert_eq!(labels(&entries), ["mine 10"]);
        assert_eq!(fs::read_to_string(&path).unwrap(), found);
    }
}

/// A file of a later version is reported by every collection that finds it, also one whose
/// fetch fails; once per collection, however many runs it writes.
#[tokio::test]
async fn a_later_version_file_is_reported_once_per_collection() {
    let (_dir, path) = cache_path();
    let newer = format!(r#"{{"version":{},"days":{{}}}}"#, super::FILE_VERSION + 1);
    fs::write(&path, &newer).unwrap();
    let count = || {
        crate::diagnostics::entries()
            .into_iter()
            .filter(|entry| {
                entry.message.contains(&path.display().to_string())
                    && entry.message.contains("later version")
            })
            .count()
    };

    let failed = collect_with_day_cache_at(
        &path,
        &ConnectionId::new(Source::Jira, "default"),
        &range(10, 10),
        POLICY,
        || at(20, 9),
        |_| async { Err(anyhow::anyhow!("offline")) },
    )
    .await;
    assert!(failed.is_err());
    assert_eq!(count(), 1);

    // Days 10 and 12 are two runs: still one warning.
    let calls = RefCell::new(Vec::new());
    let options = range(10, 12);
    let path_ref = &path;
    collect_with_day_cache_at(
        path_ref,
        &ConnectionId::new(Source::Jira, "default"),
        &options,
        POLICY,
        || at(20, 9),
        |span| {
            let calls = &calls;
            async move {
                calls.borrow_mut().push(span.utc_days());
                Ok(Vec::new())
            }
        },
    )
    .await
    .unwrap();
    assert_eq!(count(), 2);
    assert_eq!(fs::read_to_string(&path).unwrap(), newer);
}

#[cfg(unix)]
mod unix {
    use std::{fs, os::unix::fs::PermissionsExt, path::Path};

    use super::{at, cache_path, collect_tagged, labels, range, stored};
    use crate::{diagnostics, model::CollectOptions};

    fn set_mode(path: &Path, mode: u32) {
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
    }

    /// The file cannot be read again under the lock after another writer added a day: nothing
    /// is published, that day stays, and the request still gets its whole range.
    #[tokio::test]
    async fn a_file_unreadable_under_the_lock_is_left_as_it_is() {
        if unsafe { libc::geteuid() } == 0 {
            eprintln!("skipped: root reads a file of mode 000");
            return;
        }
        let (dir, path) = cache_path();
        // What the other writer leaves: its own day 20.
        let other = dir.path().join("other.json");
        collect_tagged(&other, range(20, 20), at(20, 9), "other")
            .await
            .unwrap();
        let written = fs::read(&other).unwrap();

        let entries = super::collect_with_day_cache_at(
            &path,
            &crate::model::ConnectionId::new(crate::model::Source::Jira, "default"),
            &range(10, 11),
            super::POLICY,
            || at(20, 9),
            |span: CollectOptions| {
                let (path, written) = (&path, &written);
                async move {
                    fs::write(path, written).unwrap();
                    set_mode(path, 0o000);
                    let (first, last) = span.utc_days();
                    Ok(super::super::days(first, last)
                        .map(|d| super::labelled("mine", super::day_number(d)))
                        .collect())
                }
            },
        )
        .await
        .unwrap();
        set_mode(&path, 0o600);

        assert_eq!(labels(&entries), ["mine 10", "mine 11"]);
        assert_eq!(fs::read(&path).unwrap(), written);
        assert_eq!(stored(&path, range(20, 20)).await, ["other 20"]);
    }

    /// A folder other accounts may write to gets no cache file; the warning is given once and
    /// later collections there skip the cache.
    #[tokio::test]
    async fn a_folder_that_cannot_keep_files_private_gets_no_cache_file() {
        let dir = tempfile::tempdir().unwrap();
        let shared = dir.path().join("shared");
        fs::create_dir(&shared).unwrap();
        set_mode(&shared, 0o777);
        let path = shared.join("jira-default.json");

        for _ in 0..2 {
            let entries = collect_tagged(&path, range(10, 10), at(20, 9), "x")
                .await
                .unwrap();
            assert_eq!(labels(&entries), ["x 10"]);
        }

        let names: Vec<_> = fs::read_dir(&shared)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        assert!(names.is_empty(), "{names:?}");
        let location = fs::canonicalize(&shared).unwrap().display().to_string();
        let warnings = diagnostics::entries()
            .into_iter()
            .filter(|entry| {
                entry.message
                    == format!(
                        "{location} cannot keep cached activity private; collecting without a cache"
                    )
            })
            .count();
        assert_eq!(warnings, 1);
    }

    /// A cache file in a folder that stopped being private is not trusted, even when all of
    /// its days are fresh: the range is fetched and nothing is written.
    #[tokio::test]
    async fn a_cache_file_in_a_folder_that_became_shared_is_not_read() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("cache");
        fs::create_dir(&folder).unwrap();
        let path = folder.join("jira-default.json");
        collect_tagged(&path, range(10, 10), at(20, 9), "old")
            .await
            .unwrap();
        let before = fs::read(&path).unwrap();
        set_mode(&folder, 0o777);

        let entries = collect_tagged(&path, range(10, 10), at(20, 9), "new")
            .await
            .unwrap();

        assert_eq!(labels(&entries), ["new 10"]);
        assert_eq!(fs::read(&path).unwrap(), before);
    }

    /// The verdict is not kept: once the folder is private again, collections are cached.
    #[tokio::test]
    async fn a_repaired_folder_is_cached_again() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("cache");
        fs::create_dir(&folder).unwrap();
        set_mode(&folder, 0o777);
        let path = folder.join("jira-default.json");
        collect_tagged(&path, range(10, 10), at(20, 9), "x")
            .await
            .unwrap();
        assert!(!path.exists());

        set_mode(&folder, 0o700);
        collect_tagged(&path, range(10, 10), at(20, 9), "x")
            .await
            .unwrap();

        assert!(path.exists());
    }

    /// A writer whose days all lose to the stored ones publishes nothing.
    #[tokio::test]
    async fn a_writer_that_changes_nothing_leaves_the_file_alone() {
        use std::os::unix::fs::MetadataExt;

        let (_dir, path) = cache_path();
        let path_ref = &path;
        let inode = std::cell::Cell::new(0);
        super::collect_with_day_cache_at(
            &path,
            &crate::model::ConnectionId::new(crate::model::Source::Jira, "default"),
            &range(10, 10),
            super::POLICY,
            || at(20, 9),
            |span: CollectOptions| {
                let inode = &inode;
                async move {
                    collect_tagged(path_ref, range(10, 10), at(20, 9), "b")
                        .await
                        .unwrap();
                    inode.set(fs::metadata(path_ref).unwrap().ino());
                    let (first, last) = span.utc_days();
                    Ok(super::super::days(first, last)
                        .map(|d| super::labelled("a", super::day_number(d)))
                        .collect())
                }
            },
        )
        .await
        .unwrap();

        assert_eq!(fs::metadata(&path).unwrap().ino(), inode.get());
    }

    /// A folder made shared while the writer waits for the lock is not read or written once the
    /// writer has the lock.
    #[tokio::test]
    async fn a_folder_made_shared_during_the_lock_wait_is_not_read() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("cache");
        fs::create_dir(&folder).unwrap();
        let path = folder.join("jira-default.json");
        let held = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(folder.join("jira-default.json.lock"))
            .unwrap();
        held.lock().unwrap();
        let fetched = std::sync::atomic::AtomicBool::new(false);
        let planted = r#"{"version":"planted"}"#;

        let connection = crate::model::ConnectionId::new(crate::model::Source::Jira, "default");
        let options = range(10, 10);
        let collect = super::collect_with_day_cache_at(
            &path,
            &connection,
            &options,
            super::POLICY,
            || at(20, 9),
            |span: CollectOptions| {
                let fetched = &fetched;
                async move {
                    fetched.store(true, std::sync::atomic::Ordering::SeqCst);
                    let (first, last) = span.utc_days();
                    Ok(super::super::days(first, last)
                        .map(|d| super::labelled("mine", super::day_number(d)))
                        .collect())
                }
            },
        );
        let other_writer = async {
            while !fetched.load(std::sync::atomic::Ordering::SeqCst) {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
            // The writer has checked the folder and waits for the lock.
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            set_mode(&folder, 0o777);
            fs::write(&path, planted).unwrap();
            held.unlock().unwrap();
        };
        let (entries, ()) = tokio::join!(collect, other_writer);

        assert_eq!(labels(&entries.unwrap()), ["mine 10"]);
        assert_eq!(fs::read_to_string(&path).unwrap(), planted);
        let read = diagnostics::entries().into_iter().any(|entry| {
            entry.message.contains(&path.display().to_string())
                && entry.message.contains("cannot be read")
        });
        assert!(!read, "the planted file was read");
    }
}

/// The cache file gets no ACL entry from a folder whose ACL grants everyone read to what is
/// created inside (step 1's effective-ACL test, for the cache).
#[cfg(target_os = "macos")]
#[tokio::test]
async fn a_cache_file_inherits_no_acl_entry_on_macos() {
    use std::os::unix::fs::PermissionsExt;

    use crate::platform::private_file::{acl_entry_count, readable_by_others};

    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("cache");
    fs::create_dir(&folder).unwrap();
    let status = std::process::Command::new("/bin/chmod")
        .arg("+a")
        .arg("everyone allow read,file_inherit,directory_inherit")
        .arg(&folder)
        .status()
        .unwrap();
    assert!(status.success());
    let path = folder.join("jira-default.json");

    collect_tagged(&path, range(10, 10), at(20, 9), "x")
        .await
        .unwrap();

    assert_eq!(acl_entry_count(&path).unwrap(), 0);
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(!readable_by_others(&path).unwrap());
}
