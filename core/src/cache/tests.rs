use std::{
    cell::RefCell,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use chrono::{DateTime, Local, NaiveDate, TimeZone};

use super::{CachePolicy, collect_with_day_cache_at};
use crate::model::{ActivityEntry, CollectOptions, Source, SystemEntry, SystemEntryType};

const POLICY: CachePolicy = CachePolicy {
    recent_ttl: Duration::from_secs(5 * 60),
    settled_ttl: Duration::from_secs(7 * 24 * 60 * 60),
};

fn day(d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, d).unwrap()
}

fn at(d: u32, hour: u32) -> DateTime<Local> {
    Local
        .from_local_datetime(&day(d).and_hms_opt(hour, 0, 0).unwrap())
        .unwrap()
}

fn range(since: u32, until: u32) -> CollectOptions {
    CollectOptions {
        since: day(since),
        until: day(until),
    }
}

fn entry(d: u32) -> ActivityEntry {
    ActivityEntry::System(SystemEntry {
        source: "system".to_string(),
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

/// Runs a cached collect whose fetch returns one entry per day of the widened range and
/// records which range it was asked for.
async fn collect(
    path: &Path,
    options: CollectOptions,
    now: DateTime<Local>,
    calls: &RefCell<Vec<(NaiveDate, NaiveDate)>>,
) -> Vec<ActivityEntry> {
    collect_with_day_cache_at(
        path,
        &Source::Jira,
        &options,
        POLICY,
        now,
        |span| async move {
            calls.borrow_mut().push((span.since, span.until));
            let entries = super::days(span.since, span.until)
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
async fn fetches_only_the_missing_days_widened_by_one_day() {
    let (_dir, path) = cache_path();
    let calls = RefCell::new(Vec::new());

    collect(&path, range(10, 12), at(20, 9), &calls).await;
    let widened = collect(&path, range(10, 15), at(20, 9), &calls).await;

    assert_eq!(
        labels(&widened),
        ["day 10", "day 11", "day 12", "day 13", "day 14", "day 15"]
    );
    assert_eq!(calls.borrow()[1], (day(12), day(16)));
}

#[tokio::test]
async fn refreshes_recent_days_but_keeps_settled_ones() {
    let (_dir, path) = cache_path();
    let calls = RefCell::new(Vec::new());

    // Fetched on the 20th at 09:00: days 10..19 are settled, the 20th is still running.
    collect(&path, range(10, 20), at(20, 9), &calls).await;
    collect(&path, range(10, 20), at(20, 10), &calls).await;

    assert_eq!(calls.borrow()[1], (day(19), day(21)));
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
async fn drops_entries_outside_the_requested_days() {
    let (_dir, path) = cache_path();
    let calls = RefCell::new(Vec::new());

    // The fetch is widened to the 9th and 13th; those entries must not leak into the result.
    let entries = collect(&path, range(10, 12), at(20, 9), &calls).await;

    assert_eq!(labels(&entries), ["day 10", "day 11", "day 12"]);
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
        &Source::Jira,
        &range(10, 10),
        POLICY,
        at(20, 9),
        |_| async { Err(anyhow::anyhow!("offline")) },
    )
    .await;

    assert_eq!(result.unwrap_err().to_string(), "offline");
}
