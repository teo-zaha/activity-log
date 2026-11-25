mod record;
#[cfg(test)]
mod tests;

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, PoisonError},
};

use anyhow::Result;
use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    cache::{PRUNE_AFTER, state_file},
    model::{RemoteIdentity, figma::FigmaFolderId},
    platform::private_file::{Folder, OwnerOnly, create_private_folder, open_folder},
    provider::{
        figma::scope::{FigmaFileKey, FigmaTeamId},
        provider_context::ProviderContext,
    },
};

use record::{Access, AccessCheck, FileRecord, TeamSighting};

const VERSION: u32 = 1;

/// Remembered file keys for one connection and principal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Book {
    version: u32,
    #[serde(default)]
    tab_snapshot_at: Option<DateTime<Utc>>,
    files: BTreeMap<FigmaFileKey, FileRecord>,
    #[serde(default)]
    teams: BTreeMap<FigmaTeamId, TeamFolders>,
}

/// Order by observation time, then by the complete set for equally timed observations.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub(super) struct TeamFolders {
    pub(super) discovered_at: DateTime<Utc>,
    pub(super) folders: BTreeSet<FigmaFolderId>,
}

impl Default for Book {
    fn default() -> Self {
        Self {
            version: VERSION,
            tab_snapshot_at: None,
            files: BTreeMap::new(),
            teams: BTreeMap::new(),
        }
    }
}

impl Book {
    fn merge(&mut self, other: Self) {
        self.tab_snapshot_at = self.tab_snapshot_at.max(other.tab_snapshot_at);
        for (file, record) in other.files {
            self.files.entry(file).or_default().merge(record);
        }
        for (team, folders) in other.teams {
            self.record_team(team, folders);
        }
    }

    fn record_team(&mut self, team: FigmaTeamId, folders: TeamFolders) {
        if self.teams.get(&team).is_none_or(|known| &folders > known) {
            self.teams.insert(team, folders);
        }
    }

    fn prune(&mut self, now: DateTime<Utc>) {
        let cutoff = retention_cutoff(now);
        self.teams.retain(|_, team| team.discovered_at >= cutoff);
        self.files.retain(|_, record| {
            record
                .team_sightings
                .retain(|_, sighting| sighting.seen_at >= cutoff);
            record.keep_since(cutoff)
        });
    }

    fn observe_tabs(
        &mut self,
        sightings: Vec<FigmaFileKey>,
        seen_at: Option<DateTime<Utc>>,
        pasted: &BTreeSet<FigmaFileKey>,
    ) {
        let Some(seen_at) = seen_at.filter(|&seen| Some(seen) > self.tab_snapshot_at) else {
            return;
        };
        let previous = self.tab_snapshot_at.replace(seen_at);
        for key in sightings {
            if pasted.contains(&key) && !self.files.contains_key(&key) {
                continue;
            }
            let record = self.files.entry(key).or_default();
            if record.last_tab == Some(seen_at) {
                continue;
            }
            if previous.is_some() && record.last_tab != previous {
                record.last_reappearance = Some(seen_at);
            }
            record.last_tab = Some(seen_at);
        }
    }
}

fn retention_cutoff(now: DateTime<Utc>) -> DateTime<Utc> {
    now - TimeDelta::from_std(PRUNE_AFTER).expect("the cache pruning age fits")
}

#[derive(Debug)]
pub(crate) enum BookError {
    Newer(PathBuf),
    Unreadable(PathBuf),
    ReadFailed { path: PathBuf, cause: anyhow::Error },
    Failed { path: PathBuf, cause: anyhow::Error },
    WorkerStopped(tokio::task::JoinError),
}

impl fmt::Display for BookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Newer(path) => write!(
                f,
                "Update Activity Log to read the Figma file book at {}",
                path.display()
            ),
            Self::Unreadable(path) => write!(
                f,
                "Could not read the Figma file book. Delete {} to start over.",
                path.display()
            ),
            Self::ReadFailed { path, .. } => {
                write!(
                    f,
                    "Could not read the Figma file book at {}",
                    path.display()
                )
            }
            Self::Failed { path, .. } => write!(
                f,
                "Could not update the Figma file book at {}",
                path.display()
            ),
            Self::WorkerStopped(_) => f.write_str("Figma file book work stopped unexpectedly"),
        }
    }
}

impl std::error::Error for BookError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ReadFailed { cause, .. } | Self::Failed { cause, .. } => Some(cause.as_ref()),
            Self::WorkerStopped(cause) => Some(cause),
            _ => None,
        }
    }
}

/// One collection's book, reachable from every chunk. The memory copy is used on volumes that
/// cannot keep the book private.
pub(super) struct FileBook {
    path: Option<PathBuf>,
    scope: String,
    memory: Mutex<Book>,
}

impl FileBook {
    pub(super) fn new(path: Option<PathBuf>, scope: String) -> Self {
        Self {
            path,
            scope,
            memory: Mutex::default(),
        }
    }

    pub(super) async fn begin_chunk(
        self: &Arc<Self>,
        sightings: Vec<FigmaFileKey>,
        seen_at: Option<DateTime<Utc>>,
        pasted: BTreeSet<FigmaFileKey>,
        configured_teams: BTreeSet<FigmaTeamId>,
        start: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<ChunkFiles> {
        self.edit(move |book| {
            book.observe_tabs(sightings, seen_at, &pasted);
            book.prune(now);
            let cutoff = retention_cutoff(now);
            let mut chunk = ChunkFiles::default();
            for (key, record) in &mut book.files {
                let in_scope = record.tab_activity_since(cutoff)
                    || record.team_sightings.iter().any(|(team, sighting)| {
                        configured_teams.contains(team) && sighting.last_modified >= start
                    });
                if pasted.contains(key) || !in_scope {
                    continue;
                }
                if record.claim(now) {
                    chunk.read.push(DiscoveredFile {
                        key: key.clone(),
                        last_seen: record.last_tab,
                    });
                } else {
                    chunk.lost.push(key.clone());
                }
            }
            chunk
        })
        .await
    }

    pub(super) async fn team_folders(
        self: &Arc<Self>,
        team: FigmaTeamId,
        now: DateTime<Utc>,
    ) -> Result<Option<TeamFolders>> {
        self.edit(move |book| {
            book.teams
                .get(&team)
                .filter(|known| known.discovered_at >= retention_cutoff(now))
                .cloned()
        })
        .await
    }

    pub(super) async fn remember_team(
        self: &Arc<Self>,
        team: FigmaTeamId,
        discovered: Option<TeamFolders>,
        sighted: BTreeMap<FigmaFileKey, DateTime<Utc>>,
        seen_at: DateTime<Utc>,
    ) -> Result<()> {
        self.edit(move |book| {
            if let Some(discovered) = discovered {
                book.record_team(team.clone(), discovered);
            }
            for (key, last_modified) in sighted {
                book.files.entry(key).or_default().observe_team(
                    team.clone(),
                    TeamSighting {
                        seen_at,
                        last_modified,
                    },
                );
            }
            book.prune(seen_at);
        })
        .await
    }

    pub(super) async fn finish_chunk(
        self: &Arc<Self>,
        observations: Vec<FileObservation>,
        now: DateTime<Utc>,
    ) -> Result<()> {
        if observations.is_empty() {
            return Ok(());
        }
        self.edit(move |book| {
            for observation in observations {
                let (key, record) = observation.into_record(now);
                book.files.entry(key).or_default().merge(record);
            }
            book.prune(now);
        })
        .await
    }

    async fn edit<T: Send + 'static>(
        self: &Arc<Self>,
        change: impl FnOnce(&mut Book) -> T + Send + 'static,
    ) -> Result<T> {
        let this = self.clone();
        state_file::blocking(move || {
            let mut memory = this.memory.lock().unwrap_or_else(PoisonError::into_inner);
            let Some(path) = &this.path else {
                return Ok(change(&mut memory));
            };
            match this.edit_disk(path, &mut memory, change) {
                Ok(value) => Ok(value),
                Err(error) if error.is::<BookError>() => Err(error),
                Err(cause) => Err(BookError::Failed {
                    path: path.clone(),
                    cause,
                }
                .into()),
            }
        })
        .await
        .map_err(BookError::WorkerStopped)?
    }

    fn edit_disk<T>(
        &self,
        path: &Path,
        memory: &mut Book,
        change: impl FnOnce(&mut Book) -> T,
    ) -> Result<T> {
        let (dir, name) = state_file::path_parts(path)?;
        if !dir.exists() {
            create_private_folder(dir)?;
        }
        let folder = open_folder(dir)?;
        if !state_file::is_private(&folder, &self.scope)? {
            return Ok(change(memory));
        }
        let _lock = state_file::lock(&folder, name)?;
        if !state_file::is_private(&folder, &self.scope)? {
            return Ok(change(memory));
        }
        let loaded = read_book(&folder, name, path)?;
        let mut book = loaded.clone();
        book.merge(memory.clone());
        let outcome = change(&mut book);
        if book == loaded || state_file::publish(&folder, name, &book, &self.scope)? {
            *memory = Book::default();
        } else {
            *memory = book;
        }
        Ok(outcome)
    }
}

pub(crate) fn path(context: &ProviderContext, principal: &RemoteIdentity) -> Option<PathBuf> {
    context.identity_state_path(principal, "files.json")
}

/// The startup check reads the book without reserving retries or changing retention.
pub(crate) async fn has_recent_tab_file(path: PathBuf) -> Result<bool, BookError> {
    state_file::blocking(move || {
        let check = || -> Result<bool> {
            let (dir, name) = state_file::path_parts(&path)?;
            let folder = match open_folder(dir) {
                Ok(folder) => folder,
                Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
                Err(error) => return Err(error.into()),
            };
            if !state_file::is_private(&folder, "figma")? {
                return Ok(false);
            }
            let cutoff = retention_cutoff(Utc::now());
            Ok(read_book(&folder, name, &path)?
                .files
                .values()
                .any(|record| record.tab_activity_since(cutoff)))
        };
        check().map_err(|cause| match cause.downcast::<BookError>() {
            Ok(error) => error,
            Err(cause) => BookError::ReadFailed { path, cause },
        })
    })
    .await
    .map_err(BookError::WorkerStopped)?
}

fn read_book(folder: &Folder, name: &str, path: &Path) -> Result<Book> {
    Ok(match folder.read_owner_only(name)? {
        None => Book::default(),
        Some(OwnerOnly::Exposed) => return Err(BookError::Unreadable(path.to_owned()).into()),
        Some(OwnerOnly::Private(bytes)) => match state_file::decode(&bytes, VERSION) {
            state_file::Decoded::Current(book) => book,
            state_file::Decoded::Older => Book::default(),
            state_file::Decoded::Newer => return Err(BookError::Newer(path.to_owned()).into()),
            state_file::Decoded::Unreadable(_) => {
                return Err(BookError::Unreadable(path.to_owned()).into());
            }
        },
    })
}

#[derive(Default)]
pub(super) struct ChunkFiles {
    pub(super) read: Vec<DiscoveredFile>,
    pub(super) lost: Vec<FigmaFileKey>,
}

/// A file from the book that is eligible for this chunk.
pub(super) struct DiscoveredFile {
    pub(super) key: FigmaFileKey,
    pub(super) last_seen: Option<DateTime<Utc>>,
}

pub(super) struct FileObservation {
    pub(super) file: DiscoveredFile,
    pub(super) outcome: Observation,
}

impl FileObservation {
    fn into_record(self, now: DateTime<Utc>) -> (FigmaFileKey, FileRecord) {
        let mut record = FileRecord {
            last_tab: self.file.last_seen,
            ..FileRecord::default()
        };
        let access = match self.outcome {
            Observation::Read {
                observed_at,
                contributed,
            } => {
                record.last_contribution = contributed.then_some(now);
                AccessCheck {
                    observed_at,
                    state: Access::Readable,
                }
            }
            Observation::Lost { observed_at } => AccessCheck {
                observed_at,
                state: Access::Lost,
            },
        };
        record.access = Some(access);
        (self.file.key, record)
    }
}

pub(super) enum Observation {
    Read {
        observed_at: DateTime<Utc>,
        contributed: bool,
    },
    Lost {
        observed_at: DateTime<Utc>,
    },
}
