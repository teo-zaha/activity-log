//! Per-day disk cache for providers that call remote APIs.
//!
//! A provider opts in by declaring a [`CachePolicy`]; the provider runtime then serves days it
//! already has and asks only for missing or stale days, in bounded chunks from newest to oldest.
//! One file per connection and principal (the scope); a connection whose check named no
//! principal is not cached. Writers fetch without a lock, then merge only the days they
//! fetched into the file as it is under its lock, and publish it as a new owner-only file.
//! Days are UTC calendar days, so the cache means the same thing in every timezone; the
//! requested range is cut out of them exactly.
//! Days that ended before they were fetched are "settled" and kept much longer than recent
//! days, so widening the date range or reopening the app costs one small request.

use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    fs::{File, TryLockError},
    future::Future,
    io,
    num::NonZeroUsize,
    path::{Path, PathBuf},
    sync::{Mutex, PoisonError},
    time::{Duration, Instant},
};

use anyhow::Result;
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    diagnostics,
    model::{ActivityEntry, CollectOptions, ConnectionId},
    platform::private_file::{
        Folder, PrivateFileError, ReplaceError, create_private_folder, open_folder,
    },
};

#[cfg(test)]
mod tests;

/// Bump when the file layout changes; older files are then ignored and rewritten.
const FILE_VERSION: u32 = 7;
/// A day counts as settled once it was fetched at least this long after it ended, so late
/// writes (clock skew, slow API indexing) are not frozen into the cache.
const SETTLE_GRACE: Duration = Duration::from_secs(60 * 60);
/// Days not refreshed for this long are dropped from the file.
const PRUNE_AFTER: Duration = Duration::from_secs(60 * 24 * 60 * 60);
const DEFAULT_MAX_RUN_DAYS: NonZeroUsize = NonZeroUsize::new(92).unwrap();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CachePolicy {
    /// How long today, yesterday and any day fetched before it ended stay fresh.
    pub recent_ttl: Duration,
    /// How long a settled day stays fresh.
    pub settled_ttl: Duration,
    /// Maximum number of contiguous UTC days in one fetch, if the provider splits runs.
    pub max_run_days: Option<NonZeroUsize>,
}

impl CachePolicy {
    /// Default for remote APIs. `CACHE_TTL_MINUTES` overrides the recent TTL.
    pub fn api() -> Self {
        Self::api_with_settled_ttl(Duration::from_secs(7 * 24 * 60 * 60))
    }

    /// The API defaults with a source's own settled TTL, capped at the pruning age.
    pub fn api_with_settled_ttl(settled_ttl: Duration) -> Self {
        Self {
            recent_ttl: recent_ttl(env::var("CACHE_TTL_MINUTES").ok().as_deref()),
            settled_ttl: settled_ttl.min(PRUNE_AFTER),
            max_run_days: Some(DEFAULT_MAX_RUN_DAYS),
        }
    }

    /// Fetch each contiguous run at once when the provider cannot bound its remote query.
    pub fn without_run_chunks(self) -> Self {
        Self {
            max_run_days: None,
            ..self
        }
    }
}

/// `minutes` as a TTL; five minutes when it is not a number of minutes a duration can hold.
fn recent_ttl(minutes: Option<&str>) -> Duration {
    minutes
        .and_then(|value| value.trim().parse::<u64>().ok())
        .and_then(|minutes| minutes.checked_mul(60))
        .map_or(Duration::from_secs(5 * 60), Duration::from_secs)
}

/// Why a connection with a cache policy is collected without its cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Uncached {
    /// The startup check did not name the token's owner, so days of one principal could be
    /// served to another.
    Unverified,
    /// This URL setting holds a user name, password, query or fragment.
    Url(&'static str),
}

impl Uncached {
    pub(crate) fn log(self, scope: &str) {
        match self {
            Self::Unverified => diagnostics::info(
                scope,
                "Not cached: the check did not say whose token this is",
            ),
            Self::Url(key) => diagnostics::warn(
                scope,
                format!(
                    "Not cached: the URL holds credentials ({key} has a user name, password, query or fragment)"
                ),
            ),
        }
    }
}

/// Collects `options` for a connection, reusing cached days in `dir` and fetching only the
/// stale ones.
///
/// `scope` separates caches of different principals and settings behind the same connection
/// name. `fetch` receives contiguous whole UTC days at a time, up to the policy's maximum,
/// newest chunk first.
pub(crate) async fn collect_with_day_cache<F, Fut>(
    dir: &Path,
    connection: &ConnectionId,
    scope: u64,
    options: &CollectOptions,
    policy: CachePolicy,
    fetch: F,
) -> Result<Vec<ActivityEntry>>
where
    F: Fn(CollectOptions) -> Fut,
    Fut: Future<Output = Result<Vec<ActivityEntry>>>,
{
    let path = dir.join(file_name(connection, scope));
    collect_with_day_cache_at(&path, connection, options, policy, Utc::now, fetch).await
}

pub(crate) fn file_name(connection: &ConnectionId, scope: u64) -> String {
    format!(
        "{}-{}-{scope:016x}.json",
        connection.source.as_ref(),
        connection.name
    )
}

/// [`collect_with_day_cache`] for the file at `path`. `clock` gives the time freshness is
/// judged at (read once) and the start of each chunk's fetch.
pub(crate) async fn collect_with_day_cache_at<C, F, Fut>(
    path: &Path,
    connection: &ConnectionId,
    options: &CollectOptions,
    policy: CachePolicy,
    clock: C,
    fetch: F,
) -> Result<Vec<ActivityEntry>>
where
    C: Fn() -> DateTime<Utc>,
    F: Fn(CollectOptions) -> Fut,
    Fut: Future<Output = Result<Vec<ActivityEntry>>>,
{
    let scope = connection.to_string();
    let now = clock();
    let loaded = blocking({
        let (path, scope) = (path.to_path_buf(), scope.clone());
        move || load(&path, &scope)
    })
    .await
    .flatten();
    let Some(mut file) = loaded else {
        return fetch(*options).await;
    };

    let (first_day, last_day) = options.utc_days();
    let requested: Vec<NaiveDate> = days(first_day, last_day).collect();
    let chunks: Vec<_> = requested
        .rsplit(|day| file.is_fresh(*day, policy, now))
        .flat_map(|run| {
            let size = policy
                .max_run_days
                .map_or(run.len().max(1), NonZeroUsize::get);
            run.rchunks(size)
        })
        .map(|chunk| (chunk[0], chunk[chunk.len() - 1]))
        .collect();
    let Some(&(_, newest)) = chunks.first() else {
        diagnostics::info(
            &scope,
            format!("All {} days served from cache", requested.len()),
        );
        return Ok(file.entries_for(options));
    };

    for &(first, last) in &chunks {
        let (Some(start), Some(end)) = (start_of_day(first), end_of_day(last)) else {
            anyhow::bail!("{first} to {last} is out of range");
        };
        let started = clock();
        let entries = fetch(CollectOptions { start, end }).await?;
        let fetched = fetched_days(first, last, entries, started);
        // Each chunk is written as soon as it is in, so a later fetch that fails or is cut off by
        // the connection's deadline does not cost the ones before it.
        let stored = blocking({
            let (path, fetched, scope) = (path.to_path_buf(), fetched.clone(), scope.clone());
            move || store(&path, fetched, now, &scope)
        })
        .await;
        if let Some(Err(err)) = stored {
            diagnostics::warn(
                &scope,
                format!("Could not write cache {}: {err:#}", path.display()),
            );
        }
        file.days.extend(fetched);
    }

    let stale: usize = chunks
        .iter()
        .map(|&(first, last)| days(first, last).count())
        .sum();
    let oldest = chunks.last().expect("a chunk exists").0;
    diagnostics::info(
        &scope,
        format!(
            "Fetched {stale} days across {} chunks from {oldest} to {newest}. {} days from cache",
            chunks.len(),
            requested.len() - stale
        ),
    );
    Ok(file.entries_for(options))
}

/// Runs `work` on the blocking pool, so cache I/O never holds up an async worker; `None` if it
/// panicked.
async fn blocking<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    tokio::task::spawn_blocking(work).await.ok()
}

/// The days `first..=last` as fetched at `started`, each holding the fetched entries of its
/// UTC day.
fn fetched_days(
    first: NaiveDate,
    last: NaiveDate,
    fetched: Vec<ActivityEntry>,
    started: DateTime<Utc>,
) -> BTreeMap<NaiveDate, CachedDay> {
    let fetched_at = started.timestamp_millis();
    let empty = || CachedDay {
        fetched_at,
        entries: Vec::new(),
    };
    let mut days: BTreeMap<NaiveDate, CachedDay> =
        days(first, last).map(|day| (day, empty())).collect();

    for entry in fetched {
        // Outside the run: the widened fetch overlaps neighbouring days, which are either
        // cached already or outside the requested range.
        if let Some(cached) = days.get_mut(&entry.timestamp().utc().date_naive()) {
            cached.entries.push(entry);
        }
    }
    days
}

#[derive(Debug, Serialize, Deserialize)]
struct CacheFile {
    version: u32,
    days: BTreeMap<NaiveDate, CachedDay>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedDay {
    /// Unix milliseconds.
    fetched_at: i64,
    /// Each entry as it is sent: its `source` says its kind.
    entries: Vec<ActivityEntry>,
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

    /// Takes the fetched days a parallel writer has not stored more recently: a stored day
    /// fetched at the same instant or later is kept, unless its date is one no fetch can have
    /// started at (out of range, or after `wall`, the time of this write). Whether anything
    /// changed.
    fn merge(&mut self, fetched: BTreeMap<NaiveDate, CachedDay>, wall: DateTime<Utc>) -> bool {
        let mut changed = false;
        for (day, cached) in fetched {
            let keep = self.days.get(&day).is_some_and(|stored| {
                stored.fetched_at >= cached.fetched_at
                    && Utc
                        .timestamp_millis_opt(stored.fetched_at)
                        .single()
                        .is_some_and(|at| at <= wall)
            });
            if !keep {
                self.days.insert(day, cached);
                changed = true;
            }
        }
        changed
    }

    /// Drops days not refreshed for [`PRUNE_AFTER`]; whether it dropped any.
    fn prune(&mut self, now: DateTime<Utc>) -> bool {
        let cutoff = now.timestamp_millis() - PRUNE_AFTER.as_millis() as i64;
        let before = self.days.len();
        self.days.retain(|_, cached| cached.fetched_at >= cutoff);
        self.days.len() != before
    }

    fn entries_for(&self, options: &CollectOptions) -> Vec<ActivityEntry> {
        let (first, last) = options.utc_days();
        self.days
            .range(first..=last)
            .flat_map(|(_, cached)| cached.entries.iter().cloned())
            .filter(|entry| options.contains(entry.timestamp().utc()))
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

/// The cache file before fetching; `None` when its folder cannot keep it private, so nothing
/// read there is trusted and the connection is collected without a cache. A file that cannot
/// be read is reported and counts as empty, so every requested day is fetched.
fn load(path: &Path, scope: &str) -> Option<CacheFile> {
    let (dir, name) = match path_parts(path) {
        Ok(parts) => parts,
        Err(err) => {
            diagnostics::warn(scope, format!("Could not read cache: {err}"));
            return Some(CacheFile::default());
        }
    };
    let folder = match open_folder(dir) {
        Ok(folder) => folder,
        // Created privately by the first write.
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Some(CacheFile::default()),
        Err(err) => {
            diagnostics::warn(
                scope,
                format!("Could not read cache {}: {err}", path.display()),
            );
            return Some(CacheFile::default());
        }
    };
    if !is_private(&folder, scope) {
        return None;
    }
    match folder.read(name) {
        Ok(Some(bytes)) => match decode(&bytes) {
            Decoded::Current(file) => Some(file),
            Decoded::Older => Some(CacheFile::default()),
            Decoded::Newer => {
                diagnostics::warn(
                    scope,
                    format!(
                        "Not using cache {}: it was written by a later version of Activity Log",
                        path.display()
                    ),
                );
                Some(CacheFile::default())
            }
            Decoded::Unreadable(problem) => {
                diagnostics::warn(
                    scope,
                    format!("Ignoring unreadable cache {}: {problem}", path.display()),
                );
                Some(CacheFile::default())
            }
        },
        Ok(None) => Some(CacheFile::default()),
        Err(err) => {
            diagnostics::warn(
                scope,
                format!("Could not read cache {}: {err}", path.display()),
            );
            Some(CacheFile::default())
        }
    }
}

/// What a cache file's bytes are to this version.
enum Decoded {
    Current(CacheFile),
    /// A retired layout: its days are fetched again and the file is replaced.
    Older,
    /// A later version's file: not read, and never replaced by this version.
    Newer,
    /// Not a file this version can read: the problem, without the file's content (serde's
    /// messages can quote it).
    Unreadable(String),
}

fn decode(bytes: &[u8]) -> Decoded {
    #[derive(Deserialize)]
    struct Version {
        version: u32,
    }
    // An older file stores entries in an older shape: its version is read on its own first.
    match serde_json::from_slice::<Version>(bytes) {
        Ok(file) if file.version < FILE_VERSION => return Decoded::Older,
        Ok(file) if file.version > FILE_VERSION => return Decoded::Newer,
        _ => {}
    }
    match serde_json::from_slice::<CacheFile>(bytes) {
        Ok(file) => Decoded::Current(file),
        Err(err) => Decoded::Unreadable(format!(
            "{:?} error at line {} column {}",
            err.classify(),
            err.line(),
            err.column()
        )),
    }
}

/// Merges `fetched` into the file as it is now, under the file's lock, and publishes the result
/// as a new owner-only file. Only a missing file or one of an older version is replaced
/// wholesale; a file that cannot be read again, or one of a later version, is left as it is,
/// since replacing it with the fetched days alone would drop days others stored. Nothing is
/// written when the merge changes nothing.
fn store(
    path: &Path,
    fetched: BTreeMap<NaiveDate, CachedDay>,
    now: DateTime<Utc>,
    scope: &str,
) -> Result<()> {
    let (dir, name) = path_parts(path)?;
    if !dir.exists() {
        create_private_folder(dir)?;
    }
    let folder = open_folder(dir)?;
    if !is_private(&folder, scope) {
        return Ok(());
    }
    let _lock = lock(&folder, name)?;
    // Again under the lock: the folder may have changed during the wait.
    if !is_private(&folder, scope) {
        return Ok(());
    }
    let (mut file, replaced) = match folder.read(name)? {
        None => (CacheFile::default(), true),
        Some(bytes) => match decode(&bytes) {
            Decoded::Current(file) => (file, false),
            Decoded::Older => (CacheFile::default(), true),
            // Reported by the read before the fetch.
            Decoded::Newer => return Ok(()),
            Decoded::Unreadable(problem) => anyhow::bail!("it cannot be read ({problem})"),
        },
    };
    let merged = file.merge(fetched, Utc::now());
    let pruned = file.prune(now);
    if !(replaced || merged || pruned) {
        return Ok(());
    }
    let json = serde_json::to_vec(&file)?;

    match folder.replace(name, &json) {
        Ok(()) => Ok(()),
        Err(ReplaceError::Unprotectable { location }) => {
            warn_unprotectable(&location, scope);
            Ok(())
        }
        Err(err @ ReplaceError::Failed { .. }) => Err(err.into()),
    }
}

/// Whether `folder` can keep cached activity private; checked on every read and write, so a
/// folder that is repaired is used again. The first refusal per folder is logged.
fn is_private(folder: &Folder, scope: &str) -> bool {
    match folder.check_private() {
        Ok(()) => true,
        Err(PrivateFileError::Unprotectable { location }) => {
            warn_unprotectable(&location, scope);
            false
        }
        Err(PrivateFileError::Io(err)) => {
            diagnostics::warn(
                scope,
                format!("Could not check the cache folder: {err}; collecting without a cache"),
            );
            false
        }
    }
}

/// The cache file's lock (`{name}.lock` beside it), waiting as long as a settings writer does.
fn lock(folder: &Folder, name: &str) -> Result<File> {
    let file = folder.open_lock(&format!("{name}.lock"))?;
    let deadline = Instant::now() + crate::settings::lock_wait();
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(TryLockError::WouldBlock) if Instant::now() < deadline => {
                std::thread::sleep(LOCK_RETRY);
            }
            Err(TryLockError::WouldBlock) => anyhow::bail!("another process holds its lock"),
            Err(TryLockError::Error(err)) => return Err(err.into()),
        }
    }
}

const LOCK_RETRY: Duration = Duration::from_millis(50);

fn path_parts(path: &Path) -> io::Result<(&Path, &str)> {
    let name = path.file_name().and_then(|name| name.to_str());
    match (path.parent(), name) {
        (Some(dir), Some(name)) => Ok((dir, name)),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} is not a file path", path.display()),
        )),
    }
}

/// Folders already reported as unable to keep cached activity private, in this process: the
/// warning is given once per folder. Whether a folder is used is checked every time.
static WARNED: Mutex<BTreeSet<PathBuf>> = Mutex::new(BTreeSet::new());

fn warn_unprotectable(location: &Path, scope: &str) {
    let new = WARNED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(location.to_path_buf());
    if new {
        diagnostics::warn(
            scope,
            format!(
                "{} cannot keep cached activity private; collecting without a cache",
                location.display()
            ),
        );
    }
}

/// OS cache directory, never the working directory: an app started from the Start menu or
/// Finder runs in a folder it cannot write to. `ACTIVITY_LOG_CACHE_DIR` overrides it.
pub fn cache_dir() -> PathBuf {
    if let Some(dir) = crate::platform::dirs::env_path("ACTIVITY_LOG_CACHE_DIR") {
        return dir;
    }
    crate::platform::dirs::cache_home()
        .unwrap_or_else(env::temp_dir)
        .join(crate::profile::current().folder())
        .join("cache")
}
