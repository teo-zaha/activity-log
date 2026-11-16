//! Per-day disk cache for providers that call remote APIs.
//!
//! A provider opts in by declaring a [`CachePolicy`]; the provider runtime then serves days it
//! already has and asks the provider only for the span of days that are missing or stale.
//! Days are UTC calendar days, so the cache means the same thing in every timezone; the
//! requested range is cut out of them exactly.
//! Days that ended before they were fetched are "settled" and kept much longer than recent
//! days, so widening the date range or reopening the app costs one small request.

use std::{
    collections::BTreeMap,
    env, fs,
    future::Future,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::Result;
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    diagnostics,
    model::{
        ActivityEntry, CollectOptions, GitEntry, GitLabEntry, JiraEntry, OutlookEntry, Source,
        SystemEntry,
    },
};

#[cfg(test)]
mod tests;

/// Bump when the file layout changes; older files are then ignored and rewritten.
const FILE_VERSION: u32 = 2;
/// A day counts as settled once it was fetched at least this long after it ended, so late
/// writes (clock skew, slow API indexing) are not frozen into the cache.
const SETTLE_GRACE: Duration = Duration::from_secs(60 * 60);
/// Days not refreshed for this long are dropped from the file.
const PRUNE_AFTER: Duration = Duration::from_secs(60 * 24 * 60 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CachePolicy {
    /// How long today, yesterday and any day fetched before it ended stay fresh.
    pub recent_ttl: Duration,
    /// How long a settled day stays fresh.
    pub settled_ttl: Duration,
}

impl CachePolicy {
    /// Default for remote APIs. `CACHE_TTL_MINUTES` overrides the recent TTL.
    pub fn api() -> Self {
        let recent_minutes = env::var("CACHE_TTL_MINUTES")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(5);
        Self {
            recent_ttl: Duration::from_secs(recent_minutes * 60),
            settled_ttl: Duration::from_secs(7 * 24 * 60 * 60),
        }
    }
}

/// Collects `options` for `source`, reusing cached days and fetching only the stale span.
///
/// `scope` separates caches for different configurations of the same provider (for example
/// two GitLab accounts). `fetch` receives the whole UTC days of the stale span.
pub(crate) async fn collect_with_day_cache<F, Fut>(
    source: &Source,
    scope: u64,
    options: &CollectOptions,
    policy: CachePolicy,
    fetch: F,
) -> Result<Vec<ActivityEntry>>
where
    F: FnOnce(CollectOptions) -> Fut,
    Fut: Future<Output = Result<Vec<ActivityEntry>>>,
{
    let path = cache_dir().join(format!("{}-{scope:016x}.json", source.as_ref()));
    collect_with_day_cache_at(&path, source, options, policy, Utc::now(), fetch).await
}

pub(crate) async fn collect_with_day_cache_at<F, Fut>(
    path: &Path,
    source: &Source,
    options: &CollectOptions,
    policy: CachePolicy,
    now: DateTime<Utc>,
    fetch: F,
) -> Result<Vec<ActivityEntry>>
where
    F: FnOnce(CollectOptions) -> Fut,
    Fut: Future<Output = Result<Vec<ActivityEntry>>>,
{
    let scope = source.as_ref();
    let mut file = load(path, scope);
    let (first_day, last_day) = options.utc_days();
    let requested: Vec<NaiveDate> = days(first_day, last_day).collect();
    let stale: Vec<NaiveDate> = requested
        .iter()
        .copied()
        .filter(|day| !file.is_fresh(*day, policy, now))
        .collect();

    if let (Some(first), Some(last)) = (stale.first().copied(), stale.last().copied()) {
        let (Some(start), Some(end)) = (start_of_day(first), end_of_day(last)) else {
            anyhow::bail!("{first} to {last} is out of range");
        };
        let fetched = fetch(CollectOptions { start, end }).await?;
        file.replace_span(first, last, fetched, now, scope);
        file.prune(now);
        if let Err(err) = save(path, &file) {
            diagnostics::warn(
                scope,
                format!("Could not write cache {}: {err:#}", path.display()),
            );
        }

        let cached = requested.len() - stale.len();
        diagnostics::info(
            scope,
            format!(
                "Fetched {first} to {last} ({} days); {cached} days from cache",
                days(first, last).count()
            ),
        );
    } else {
        diagnostics::info(
            scope,
            format!("All {} days served from cache", requested.len()),
        );
    }

    Ok(file.entries_for(options))
}

#[derive(Debug, Serialize, Deserialize)]
struct CacheFile {
    version: u32,
    days: BTreeMap<NaiveDate, CachedDay>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CachedDay {
    /// Unix milliseconds.
    fetched_at: i64,
    entries: Vec<StoredEntry>,
}

/// `ActivityEntry` is untagged on the wire; the cache stores an explicit tag so entries always
/// deserialize back into the variant they came from.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "entry", rename_all = "lowercase")]
enum StoredEntry {
    Git(GitEntry),
    GitLab(GitLabEntry),
    Jira(JiraEntry),
    Outlook(OutlookEntry),
    System(SystemEntry),
}

impl From<ActivityEntry> for StoredEntry {
    fn from(entry: ActivityEntry) -> Self {
        match entry {
            ActivityEntry::Git(entry) => Self::Git(entry),
            ActivityEntry::GitLab(entry) => Self::GitLab(entry),
            ActivityEntry::Jira(entry) => Self::Jira(entry),
            ActivityEntry::Outlook(entry) => Self::Outlook(entry),
            ActivityEntry::System(entry) => Self::System(entry),
        }
    }
}

impl From<StoredEntry> for ActivityEntry {
    fn from(entry: StoredEntry) -> Self {
        match entry {
            StoredEntry::Git(entry) => Self::Git(entry),
            StoredEntry::GitLab(entry) => Self::GitLab(entry),
            StoredEntry::Jira(entry) => Self::Jira(entry),
            StoredEntry::Outlook(entry) => Self::Outlook(entry),
            StoredEntry::System(entry) => Self::System(entry),
        }
    }
}

impl Default for CacheFile {
    fn default() -> Self {
        Self {
            version: FILE_VERSION,
            days: BTreeMap::new(),
        }
    }
}

impl CacheFile {
    fn is_fresh(&self, day: NaiveDate, policy: CachePolicy, now: DateTime<Utc>) -> bool {
        let Some(cached) = self.days.get(&day) else {
            return false;
        };
        let Some(fetched_at) = Utc.timestamp_millis_opt(cached.fetched_at).single() else {
            return false;
        };
        let settled = end_of_day(day).is_some_and(|end| fetched_at >= end + SETTLE_GRACE);
        let ttl = if settled {
            policy.settled_ttl
        } else {
            policy.recent_ttl
        };
        now.signed_duration_since(fetched_at)
            .to_std()
            .is_ok_and(|age| age <= ttl)
    }

    fn replace_span(
        &mut self,
        first: NaiveDate,
        last: NaiveDate,
        fetched: Vec<ActivityEntry>,
        now: DateTime<Utc>,
        scope: &str,
    ) {
        let fetched_at = now.timestamp_millis();
        for day in days(first, last) {
            self.days.insert(
                day,
                CachedDay {
                    fetched_at,
                    entries: Vec::new(),
                },
            );
        }

        let mut undated = 0;
        for entry in fetched {
            match utc_day(entry.timestamp()) {
                Some(day) if (first..=last).contains(&day) => {
                    if let Some(cached) = self.days.get_mut(&day) {
                        cached.entries.push(entry.into());
                    }
                }
                // Outside the stale span: the widened fetch overlaps neighbouring days, which
                // are either cached already or outside the requested range.
                Some(_) => {}
                None => undated += 1,
            }
        }
        if undated > 0 {
            diagnostics::warn(
                scope,
                format!("Dropped {undated} entries with unreadable timestamps"),
            );
        }
    }

    fn prune(&mut self, now: DateTime<Utc>) {
        let cutoff = now.timestamp_millis() - PRUNE_AFTER.as_millis() as i64;
        self.days.retain(|_, cached| cached.fetched_at >= cutoff);
    }

    fn entries_for(&self, options: &CollectOptions) -> Vec<ActivityEntry> {
        let (first, last) = options.utc_days();
        self.days
            .range(first..=last)
            .flat_map(|(_, cached)| cached.entries.iter().cloned().map(ActivityEntry::from))
            .filter(|entry| instant(entry.timestamp()).is_some_and(|at| options.contains(at)))
            .collect()
    }
}

fn days(since: NaiveDate, until: NaiveDate) -> impl Iterator<Item = NaiveDate> {
    since.iter_days().take_while(move |day| *day <= until)
}

fn start_of_day(day: NaiveDate) -> Option<DateTime<Utc>> {
    Some(day.and_hms_opt(0, 0, 0)?.and_utc())
}

fn end_of_day(day: NaiveDate) -> Option<DateTime<Utc>> {
    start_of_day(day.succ_opt()?)
}

fn instant(timestamp: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(timestamp)
        .ok()
        .map(|parsed| parsed.with_timezone(&Utc))
}

fn utc_day(timestamp: &str) -> Option<NaiveDate> {
    instant(timestamp).map(|at| at.date_naive())
}

fn load(path: &Path, scope: &str) -> CacheFile {
    let Ok(raw) = fs::read_to_string(path) else {
        return CacheFile::default();
    };
    match serde_json::from_str::<CacheFile>(&raw) {
        Ok(file) if file.version == FILE_VERSION => file,
        Ok(_) => CacheFile::default(),
        Err(err) => {
            diagnostics::warn(
                scope,
                format!("Ignoring unreadable cache {}: {err}", path.display()),
            );
            CacheFile::default()
        }
    }
}

fn save(path: &Path, file: &CacheFile) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_string(file)?;
    // Write then rename, so a crash or a parallel CLI run never leaves a half-written file.
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

/// OS cache directory, never the working directory: an app started from the Start menu or
/// Finder runs in a folder it cannot write to. `ACTIVITY_LOG_CACHE_DIR` overrides it.
pub fn cache_dir() -> PathBuf {
    if let Some(dir) = env::var_os("ACTIVITY_LOG_CACHE_DIR") {
        return PathBuf::from(dir);
    }

    let home = || env::var_os("HOME").map(PathBuf::from);
    let base = if cfg!(target_os = "windows") {
        env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        home().map(|home| home.join("Library").join("Caches"))
    } else {
        env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| home().map(|home| home.join(".cache")))
    };

    base.unwrap_or_else(env::temp_dir)
        .join("activity-log")
        .join("cache")
}
