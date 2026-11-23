//! Writing `config.toml`. Every writer (the CLI, the desktop settings) goes through
//! [`edit_config`]: under an interprocess lock it reads the file again, checks the writer's
//! precondition, applies the change, checks the result can be read, and publishes it as a new
//! owner-only file that replaces the old one atomically. A file that would not change is never
//! written.

use std::{
    fs::{self, File},
    io,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use anyhow::anyhow;

use super::{
    ConfigEditError, ConfigError, ConfigFile, ContentId, NO_LOCATION, precondition, warn_if_shared,
};
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
    /// The file changed while a copy of it was edited (`config edit`); nothing was written.
    FileChanged { name: String },
    /// The change was refused; nothing was written.
    Rejected(ConfigEditError),
    /// The file as found, or the text the user edited, is not a valid configuration; nothing
    /// was written.
    Invalid(ConfigError),
    /// The location cannot hold private files; nothing was written.
    Unprotectable { location: PathBuf },
    /// Reading, locking or writing failed, or the edit stopped; the file is as it was.
    Internal(anyhow::Error),
}

impl EditError {
    /// The message for the user, naming a conflicting connection by `label` when given.
    pub fn message(&self, label: Option<&str>) -> String {
        match self {
            Self::Conflict(conflict) => conflict.message(
                label
                    .or(conflict.label.as_deref())
                    .unwrap_or(&conflict.connection.to_string()),
            ),
            other => other.to_string(),
        }
    }
}

impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoLocation => write!(f, "{NO_LOCATION}"),
            Self::Busy => write!(f, "The settings file is busy; try again."),
            Self::Conflict(_) => write!(f, "{}", self.message(None)),
            Self::FileChanged { name } => write!(f, "{name} changed while you were editing"),
            Self::Rejected(err) => write!(f, "{err}"),
            Self::Invalid(err) => write!(f, "{err}"),
            Self::Internal(err) => write!(f, "{err:#}"),
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
    change: impl FnOnce(&mut ConfigFile) -> Result<T, ConfigEditError>,
) -> Result<Edited<T>, EditError> {
    edit_config_with(lock(path)?, precondition, change)
}

type TryLock<'a> = &'a dyn Fn(&File) -> Result<(), fs::TryLockError>;

fn edit_config_with<T>(
    locked: Locked,
    precondition: Precondition,
    change: impl FnOnce(&mut ConfigFile) -> Result<T, ConfigEditError>,
) -> Result<Edited<T>, EditError> {
    let found = locked.read()?;
    let mut file = ConfigFile::parse(&locked.path, found.as_deref().unwrap_or_default())
        .map_err(EditError::Invalid)?;
    let as_read = file.to_string();
    precondition::check(&file, &precondition).map_err(EditError::Conflict)?;
    let value = change(&mut file).map_err(EditError::Rejected)?;
    // The change was accepted: a failure here is the writer's own.
    file.assign_instances()
        .map_err(|err| EditError::Internal(anyhow!(err).context("Giving connections instances")))?;
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
            EditError::Internal(anyhow!(err).context(format!("Reading {}", self.path.display())))
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
        // The writer made these bytes from text that parsed: one that does not read back is the
        // writer's failure, not invalid input.
        let file = ConfigFile::parse(&self.path, &content).map_err(|err| {
            EditError::Internal(anyhow!(err).context("The written settings do not read back"))
        })?;
        let warnings = file.connections().warnings;
        if found != Some(content.as_slice()) {
            self.publish(&content)?;
        }
        Ok(Written {
            file,
            content: ContentId::of(&content),
            warnings,
        })
    }

    /// Replaces the file with `content` through a private staging folder in the locked folder
    /// ([`Folder::replace`]). A failure leaves the old file as it was and names its step.
    fn publish(&self, content: &[u8]) -> Result<(), EditError> {
        self.folder
            .replace(&self.name, content)
            .map_err(unprotectable_or(|err| {
                EditError::Internal(err.context(format!("Writing {}", self.path.display())))
            }))
    }
}

/// [`EditError::Unprotectable`] for such a location, `other` for the rest.
pub(super) fn unprotectable_or(
    other: impl Fn(anyhow::Error) -> EditError,
) -> impl Fn(PrivateFileError) -> EditError {
    move |err| match err {
        PrivateFileError::Unprotectable { location } => EditError::Unprotectable { location },
        err @ (PrivateFileError::Io(_) | PrivateFileError::Failed { .. }) => other(anyhow!(err)),
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
        EditError::Internal(anyhow!(err).context(format!("Opening {}", parent.display())))
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
        EditError::Internal(anyhow!(err).context(format!("Opening {}", lock_path.display())))
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
                return Err(EditError::Internal(
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
        .ok_or_else(|| EditError::Internal(anyhow!("{} is not a file name", path.display())))
}

/// A link is followed to its target, which is replaced while the link stays; its folder is
/// created when missing.
pub(super) fn resolve(path: &Path) -> Result<PathBuf, EditError> {
    let is_link = fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink());
    let path = if is_link {
        fs::canonicalize(path).map_err(|err| {
            EditError::Internal(if err.kind() == io::ErrorKind::NotFound {
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
            EditError::Internal(anyhow!(err).context(format!("Creating {}", parent.display())))
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
