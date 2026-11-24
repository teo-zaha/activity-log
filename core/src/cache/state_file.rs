//! Locked, owner-only JSON state beside the day cache.

use std::{
    collections::BTreeSet,
    fs::{File, TryLockError},
    io,
    path::{Path, PathBuf},
    sync::{Mutex, PoisonError},
    time::{Duration, Instant},
};

use anyhow::Result;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{
    diagnostics,
    platform::private_file::{Folder, PrivateFileError, ReplaceError},
};

/// Runs `work` on the blocking pool, so cache I/O never holds up an async worker; `None` if it
/// panicked.
pub(crate) async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    tokio::task::spawn_blocking(work).await.ok()
}

pub(crate) fn read<T: DeserializeOwned>(
    folder: &Folder,
    name: &str,
    version: u32,
) -> io::Result<Option<Decoded<T>>> {
    folder
        .read(name)
        .map(|bytes| bytes.map(|bytes| decode(&bytes, version)))
}

/// Atomically writes owner-only state. The caller holds the file's lock.
pub(crate) fn publish<T: Serialize>(
    folder: &Folder,
    name: &str,
    state: &T,
    scope: &str,
) -> Result<()> {
    let json = serde_json::to_vec(state)?;
    match folder.replace(name, &json) {
        Ok(()) => Ok(()),
        Err(ReplaceError::Unprotectable { location }) => {
            warn_unprotectable(&location, scope);
            Ok(())
        }
        Err(err @ ReplaceError::Failed { .. }) => Err(err.into()),
    }
}

/// What a state file's bytes are to this version.
pub(crate) enum Decoded<T> {
    Current(T),
    /// A retired layout: the caller can replace it.
    Older,
    /// A later version's file: not read, and never replaced by this version.
    Newer,
    /// Not a file this version can read: the problem, without the file's content (serde's
    /// messages can quote it).
    Unreadable(String),
}

fn decode<T: DeserializeOwned>(bytes: &[u8], current_version: u32) -> Decoded<T> {
    #[derive(Deserialize)]
    struct Version {
        version: u32,
    }
    // An older file stores entries in an older shape: its version is read on its own first.
    match serde_json::from_slice::<Version>(bytes) {
        Ok(file) if file.version < current_version => return Decoded::Older,
        Ok(file) if file.version > current_version => return Decoded::Newer,
        _ => {}
    }
    match serde_json::from_slice::<T>(bytes) {
        Ok(file) => Decoded::Current(file),
        Err(err) => Decoded::Unreadable(format!(
            "{:?} error at line {} column {}",
            err.classify(),
            err.line(),
            err.column()
        )),
    }
}

/// Whether `folder` can keep cached activity private; checked on every read and write, so a
/// folder that is repaired is used again. The first refusal per folder is logged.
pub(crate) fn is_private(folder: &Folder, scope: &str) -> io::Result<bool> {
    match folder.check_private() {
        Ok(()) => Ok(true),
        Err(PrivateFileError::Unprotectable { location }) => {
            warn_unprotectable(&location, scope);
            Ok(false)
        }
        Err(PrivateFileError::Io(err)) => Err(err),
    }
}

/// The state file's lock (`{name}.lock` beside it), waiting as long as a settings writer does.
pub(crate) fn lock(folder: &Folder, name: &str) -> Result<File> {
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

pub(crate) fn path_parts(path: &Path) -> io::Result<(&Path, &str)> {
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
