//! Writing `config.toml`. Every writer (the CLI, the desktop settings) goes through
//! [`edit_config`]: under an interprocess lock it reads the file again, checks the writer's
//! precondition, applies the change, checks the result can be read, and publishes it as a new
//! owner-only file that replaces the old one atomically. A file that would not change is never
//! written.

use std::{
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use anyhow::anyhow;

use super::{ConfigFile, ContentId, NO_LOCATION, precondition, warn_if_shared};
use crate::{
    platform::private_file::{Folder, PrivateFileError, create_private_folder, open_folder},
    settings::{Conflict, Precondition},
};

#[cfg(test)]
mod tests;

/// How long a writer waits for another one before giving up.
const LOCK_WAIT: Duration = Duration::from_secs(5);
const LOCK_RETRY: Duration = Duration::from_millis(50);
/// Shortens the wait, for tests that run the CLI against a held lock.
const LOCK_WAIT_VARIABLE: &str = "ACTIVITY_LOG_LOCK_WAIT_MS";
/// The new file's name inside the staging folder.
const NEW_FILE: &str = "config.toml.new";

pub struct Edited<T> {
    pub value: T,
    /// The file as written (or as found, when nothing had to be written).
    pub file: ConfigFile,
    pub content: ContentId,
    /// Problems in single connections of the result, for the caller to log after the write.
    pub warnings: Vec<String>,
}

#[derive(Debug)]
pub enum EditError {
    /// No settings location: nothing was read or written.
    NoLocation,
    /// Another process held the lock for the whole wait.
    Busy,
    /// The precondition failed: the connection was changed, replaced or removed by another
    /// writer. Nothing was written.
    Conflict(Conflict),
    /// The change refused, or its result could not be read again; nothing was written.
    Rejected(anyhow::Error),
    /// The location cannot hold private files; nothing was written.
    Unprotectable { location: PathBuf },
    /// Reading, locking or writing failed; the file is as it was.
    Io(anyhow::Error),
}

impl EditError {
    /// The message for the user, naming a conflicting connection by `label` when given.
    pub fn message(&self, label: Option<&str>) -> String {
        match self {
            Self::Conflict(conflict) => {
                conflict.message(label.unwrap_or(&conflict.connection.to_string()))
            }
            other => other.to_string(),
        }
    }
}

impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoLocation => write!(f, "{NO_LOCATION}"),
            Self::Busy => write!(f, "The settings file is busy; try again."),
            Self::Conflict(conflict) => {
                write!(f, "{}", conflict.message(&conflict.connection.to_string()))
            }
            Self::Rejected(err) | Self::Io(err) => write!(f, "{err:#}"),
            Self::Unprotectable { location } => write!(
                f,
                "{} cannot keep the settings private; choose another location with \
                 ACTIVITY_LOG_CONFIG or --config",
                location.display()
            ),
        }
    }
}

impl std::error::Error for EditError {}

/// Edits the config file at `path` under the config lock. Callers without a location report
/// [`EditError::NoLocation`] before this is reached.
pub fn edit_config<T>(
    path: &Path,
    precondition: Precondition,
    change: impl FnOnce(&mut ConfigFile) -> anyhow::Result<T>,
) -> Result<Edited<T>, EditError> {
    edit_config_with(lock(path)?, precondition, change)
}

type TryLock<'a> = &'a dyn Fn(&File) -> Result<(), fs::TryLockError>;

fn edit_config_with<T>(
    locked: Locked,
    precondition: Precondition,
    change: impl FnOnce(&mut ConfigFile) -> anyhow::Result<T>,
) -> Result<Edited<T>, EditError> {
    let found = locked.read()?;
    let mut file = ConfigFile::parse(&locked.path, found.as_deref().unwrap_or_default())
        .map_err(|err| EditError::Io(err.into()))?;
    let as_read = file.to_string();
    precondition::check(&file, &precondition).map_err(EditError::Conflict)?;
    let value = change(&mut file).map_err(EditError::Rejected)?;
    file.assign_instances().map_err(EditError::Rejected)?;
    let serialized = file.to_string();
    // An edit that changed nothing keeps the file's own bytes: its spelling, comments and line
    // endings.
    let content = match &found {
        Some(found) if serialized == as_read => found.clone(),
        found => with_line_endings_of(serialized, found.as_deref()),
    };
    let written = locked.write_checked(found.as_deref(), content)?;
    Ok(Edited {
        value,
        file: written.file,
        content: written.content,
        warnings: written.warnings,
    })
}

/// `text` with CRLF line endings when `found` uses them throughout.
pub(super) fn with_line_endings_of(text: String, found: Option<&[u8]>) -> Vec<u8> {
    let crlf = found.is_some_and(|found| {
        let lines = found.iter().filter(|byte| **byte == b'\n').count();
        lines > 0 && found.windows(2).filter(|pair| pair == b"\r\n").count() == lines
    });
    if crlf {
        text.replace("\r\n", "\n")
            .replace('\n', "\r\n")
            .into_bytes()
    } else {
        text.into_bytes()
    }
}

/// A file the app wrote (or found as it would have written it).
pub(super) struct Written {
    pub file: ConfigFile,
    pub content: ContentId,
    pub warnings: Vec<String>,
}

/// The config lock, held until dropped, and the file it guards. The lock file, the reads and
/// the publication all go through the one folder handle taken here.
pub(super) struct Locked {
    folder: Folder,
    name: String,
    /// The config file, with a link resolved to its target, for messages.
    pub path: PathBuf,
    _lock: File,
}

impl Locked {
    /// The file's bytes, `None` when it does not exist.
    pub(super) fn read(&self) -> Result<Option<Vec<u8>>, EditError> {
        let bytes = self.folder.read(&self.name).map_err(|err| {
            EditError::Io(anyhow!(err).context(format!("Reading {}", self.path.display())))
        })?;
        if bytes.is_some() {
            warn_if_shared(&self.path);
        }
        Ok(bytes)
    }

    /// Checks that `content` reads as a config and publishes it, unless the file already holds
    /// exactly that.
    pub(super) fn write_checked(
        &self,
        found: Option<&[u8]>,
        content: Vec<u8>,
    ) -> Result<Written, EditError> {
        let file = ConfigFile::parse(&self.path, &content).map_err(|err| {
            EditError::Rejected(anyhow!(err).context("The result is not readable"))
        })?;
        let warnings = file.connections().map_err(EditError::Rejected)?.warnings;
        if found != Some(content.as_slice()) {
            self.publish(&content)?;
        }
        Ok(Written {
            file,
            content: ContentId::of(&content),
            warnings,
        })
    }

    /// Writes `content` into a new file of a private staging folder in the locked folder and
    /// moves it over the file. A failure leaves the old file as it was.
    fn publish(&self, content: &[u8]) -> Result<(), EditError> {
        let failed = |err: io::Error| {
            EditError::Io(anyhow!(err).context(format!("Writing {}", self.path.display())))
        };
        let staging = self
            .folder
            .staging(&self.name)
            .map_err(unprotectable_or(failed))?;
        let mut file = staging.create(NEW_FILE).map_err(failed)?;
        file.write_all(content).map_err(failed)?;
        file.sync_all().map_err(failed)?;
        drop(file);
        staging.publish(NEW_FILE).map_err(failed)
    }
}

/// [`EditError::Unprotectable`] for such a location, `other` for the rest.
pub(super) fn unprotectable_or(
    other: impl Fn(io::Error) -> EditError,
) -> impl Fn(PrivateFileError) -> EditError {
    move |err| match err {
        PrivateFileError::Unprotectable { location } => EditError::Unprotectable { location },
        PrivateFileError::Io(err) => other(err),
    }
}

/// Takes the config lock: `config.toml.lock` beside the (resolved) file, waiting up to 5 s for
/// another writer.
pub(super) fn lock(path: &Path) -> Result<Locked, EditError> {
    let (folder, path) = open_parent(path)?;
    lock_in(folder, path)
}

/// The lock in an open folder, for a writer that already works in it.
pub(super) fn lock_in(folder: Folder, path: PathBuf) -> Result<Locked, EditError> {
    lock_with(folder, path, &|file: &File| file.try_lock(), lock_wait())
}

/// The resolved file and its folder, held open.
pub(super) fn open_parent(path: &Path) -> Result<(Folder, PathBuf), EditError> {
    let path = resolve(path)?;
    let parent = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    let folder = open_folder(parent).map_err(|err| {
        EditError::Io(anyhow!(err).context(format!("Opening {}", parent.display())))
    })?;
    Ok((folder, path))
}

/// Only contention is retried; any other lock error is reported as such.
fn lock_with(
    folder: Folder,
    path: PathBuf,
    try_lock: TryLock,
    wait: Duration,
) -> Result<Locked, EditError> {
    let name = file_name(&path)?;
    let lock_name = format!("{name}.lock");
    let lock_path = path.with_file_name(&lock_name);
    let file = folder.open_lock(&lock_name).map_err(|err| {
        EditError::Io(anyhow!(err).context(format!("Opening {}", lock_path.display())))
    })?;
    let deadline = Instant::now() + wait;
    loop {
        match try_lock(&file) {
            Ok(()) => {
                return Ok(Locked {
                    folder,
                    name,
                    path,
                    _lock: file,
                });
            }
            Err(fs::TryLockError::WouldBlock) if Instant::now() < deadline => {
                std::thread::sleep(LOCK_RETRY);
            }
            Err(fs::TryLockError::WouldBlock) => return Err(EditError::Busy),
            Err(fs::TryLockError::Error(err)) => {
                return Err(EditError::Io(
                    anyhow!(err).context(format!("Locking {}", lock_path.display())),
                ));
            }
        }
    }
}

pub(super) fn file_name(path: &Path) -> Result<String, EditError> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string)
        .ok_or_else(|| EditError::Io(anyhow!("{} is not a file name", path.display())))
}

/// A link is followed to its target, which is replaced while the link stays; its folder is
/// created when missing.
pub(super) fn resolve(path: &Path) -> Result<PathBuf, EditError> {
    let is_link = fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink());
    let path = if is_link {
        fs::canonicalize(path).map_err(|err| {
            EditError::Io(if err.kind() == io::ErrorKind::NotFound {
                anyhow!("{} links to a file that does not exist", path.display())
            } else {
                anyhow!(err).context(format!("Resolving {}", path.display()))
            })
        })?
    } else {
        path.to_path_buf()
    };
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        && !parent.exists()
    {
        create_private_folder(parent).map_err(|err| {
            EditError::Io(anyhow!(err).context(format!("Creating {}", parent.display())))
        })?;
    }
    Ok(path)
}

/// How long a writer of a shared file (settings, cache) waits for another one.
pub(crate) fn lock_wait() -> Duration {
    lock_wait_from(std::env::var(LOCK_WAIT_VARIABLE).ok().as_deref())
}

/// The variable can only shorten the wait.
fn lock_wait_from(value: Option<&str>) -> Duration {
    value
        .and_then(|value| value.parse::<u64>().ok())
        .map_or(LOCK_WAIT, |millis| {
            Duration::from_millis(millis).min(LOCK_WAIT)
        })
}
