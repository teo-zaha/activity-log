use std::{
    collections::{BTreeMap, BTreeSet, VecDeque, btree_map::Entry},
    fmt,
    sync::Arc,
};

use anyhow::Result;
use chrono::{DateTime, TimeDelta, Utc};
use reqwest::StatusCode;

use crate::{
    diagnostics,
    model::{Source, figma::FigmaFolderId},
    provider::{
        figma::{
            api::{FigmaApi, FolderListingError},
            scope::{FigmaFileKey, FigmaTeamId},
        },
        remote_error::{ApiError, RemoteFailure},
    },
};

use super::file_book::{FileBook, TeamFolders};

#[derive(Debug)]
pub(super) struct TeamDiscoveryError {
    pub(super) position: usize,
    pub(super) cause: TeamFailure,
}

impl fmt::Display for TeamDiscoveryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Could not read Figma team list item {}", self.position)
    }
}

impl std::error::Error for TeamDiscoveryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.cause {
            TeamFailure::Request(failure) => Some(failure),
            cause => Some(cause),
        }
    }
}

#[derive(Debug)]
pub(super) enum TeamFailure {
    Request(RemoteFailure),
    InconsistentFolders,
    Paginated,
}

impl From<FolderListingError> for TeamFailure {
    fn from(error: FolderListingError) -> Self {
        match error {
            FolderListingError::Request(error) => Self::Request(RemoteFailure {
                service: FigmaApi::SERVICE.into(),
                error,
            }),
            FolderListingError::Paginated => Self::Paginated,
        }
    }
}

impl TeamFailure {
    fn folder_unavailable(&self) -> bool {
        matches!(
            self,
            Self::Request(RemoteFailure {
                error: error @ ApiError::Status {
                    status: StatusCode::FORBIDDEN | StatusCode::NOT_FOUND, ..
                }, ..
            }) if !error.is_busy()
        )
    }
}

impl fmt::Display for TeamFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Request(failure) => failure.fmt(f),
            Self::InconsistentFolders => f.write_str("Figma returned an inconsistent folder tree"),
            Self::Paginated => f.write_str("Figma returned a paged folder listing"),
        }
    }
}

impl std::error::Error for TeamFailure {}

pub(super) async fn refresh(
    api: &FigmaApi,
    book: &Arc<FileBook>,
    teams: &[(usize, FigmaTeamId)],
    recent_range: bool,
) -> Result<()> {
    let mut listed = BTreeMap::new();
    let mut invalid_keys = 0;
    for (position, team) in teams {
        let failed = |cause| TeamDiscoveryError {
            position: *position,
            cause,
        };
        let now = Utc::now();
        let saved = book.team_folders(team.clone(), now).await?;
        if !recent_range && saved.is_some() {
            continue;
        }
        if let Some(fresh) = saved.filter(|tree| now - tree.discovered_at < TimeDelta::days(1)) {
            match list_files(api, &fresh.folders, &mut listed, &mut invalid_keys).await {
                Ok(files) => {
                    book.remember_team(team.clone(), None, files, Utc::now())
                        .await?;
                    continue;
                }
                Err(cause) if cause.folder_unavailable() => {
                    // A cached folder may have been deleted or become invisible since discovery.
                }
                Err(cause) => return Err(failed(cause).into()),
            }
        }
        let folders = discover_folders(api, team).await.map_err(failed)?;
        let discovered = TeamFolders {
            folders,
            discovered_at: Utc::now(),
        };
        let files = list_files(api, &discovered.folders, &mut listed, &mut invalid_keys)
            .await
            .map_err(failed)?;
        book.remember_team(team.clone(), Some(discovered), files, Utc::now())
            .await?;
    }
    if invalid_keys > 0 {
        diagnostics::warn(
            Source::Figma.as_ref(),
            format!("Skipped invalid file keys from Figma team folders ({invalid_keys} affected)"),
        );
    }
    Ok(())
}

async fn discover_folders(
    api: &FigmaApi,
    team: &FigmaTeamId,
) -> Result<BTreeSet<FigmaFolderId>, TeamFailure> {
    let mut pending: VecDeque<_> = api
        .team_folders(team)
        .await?
        .into_iter()
        .map(|folder| (None, folder))
        .collect();
    let mut folders = BTreeSet::new();
    while let Some((parent, folder)) = pending.pop_front() {
        if folder.parent_folder_id != parent || !folders.insert(folder.id.clone()) {
            return Err(TeamFailure::InconsistentFolders);
        }
        let children = api.subfolders(&folder.id).await?;
        pending.extend(
            children
                .into_iter()
                .map(|child| (Some(folder.id.clone()), child)),
        );
    }
    Ok(folders)
}

type FileTimes = BTreeMap<FigmaFileKey, DateTime<Utc>>;

async fn list_files(
    api: &FigmaApi,
    folders: &BTreeSet<FigmaFolderId>,
    listed: &mut BTreeMap<FigmaFolderId, Vec<(FigmaFileKey, DateTime<Utc>)>>,
    invalid_keys: &mut usize,
) -> Result<FileTimes, TeamFailure> {
    let mut sightings = FileTimes::new();
    for folder in folders {
        let files = match listed.entry(folder.clone()) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                let reply = api.folder_files(entry.key()).await?;
                let files = reply
                    .files
                    .into_iter()
                    .filter_map(|file| {
                        let key = file.key.as_str().and_then(FigmaFileKey::from_key);
                        *invalid_keys += usize::from(key.is_none());
                        key.map(|key| (key, file.last_modified.utc()))
                    })
                    .collect();
                entry.insert(files)
            }
        };
        for (key, modified) in files {
            let time = sightings.entry(key.clone()).or_insert(*modified);
            *time = (*time).max(*modified);
        }
    }
    Ok(sightings)
}
