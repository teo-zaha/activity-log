use std::{
    cell::RefCell,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use chrono::{DateTime, NaiveDate, Utc};

use super::{CachePolicy, collect_with_day_cache_at};
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
        now,
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
        at(20, 9),
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
    let scope = ConnectionId::new(Source::Jira, "default").to_string();
    assert!(
        !crate::diagnostics::entries().iter().any(
            |entry| entry.scope == scope && entry.message.contains(&path.display().to_string())
        )
    );
}
