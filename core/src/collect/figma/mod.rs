pub(crate) mod file_book;
pub(crate) mod local_tabs;
mod run;
mod team_discovery;

use std::{collections::HashMap, fmt};

use chrono::{TimeDelta, Utc};

use anyhow::Result;
use futures::{StreamExt, stream};
use reqwest::Url;

use crate::{
    collect::web_link::{Unshown, web_link},
    diagnostics,
    model::{
        CollectOptions, ConnectionName, FigmaActivity, FigmaEditor, FigmaEntry, FigmaTag,
        IdentityEvidence, Source, Timestamp,
        figma::{FigmaComment, FigmaVersion},
    },
    provider::{
        figma::{
            FigmaConfig,
            api::{FigmaApi, VersionsError},
            scope::{FigmaFileKey, FigmaScope},
        },
        provider_context::ProviderContext,
        remote_api::SetupError,
        remote_error::{ApiError, RemoteFailure},
    },
};

use file_book::{DiscoveredFile, FileObservation, Observation};
pub(crate) use run::FigmaCollection;
use run::{ConnectionRun, TabSnapshot};

const FILE_CONCURRENCY: usize = 3;
const WEB_ORIGIN: &str = "https://www.figma.com";

#[derive(Debug)]
pub(crate) enum FigmaCollectError {
    NoIdentity,
    Setup(SetupError),
    FileRequest {
        file: FilePlace,
        failure: RemoteFailure,
    },
    RepeatedVersions {
        file: FilePlace,
    },
}

impl fmt::Display for FigmaCollectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoIdentity => f.write_str(
                "Figma did not say whose token this is. Activity Log cannot identify your versions or comments.",
            ),
            Self::Setup(error) => f.write_str(&error.reason(FigmaApi::SERVICE)),
            Self::FileRequest { file, .. } => write!(f, "Could not read {file}"),
            Self::RepeatedVersions { file } => {
                write!(f, "Figma repeated version history for {file}")
            }
        }
    }
}

impl std::error::Error for FigmaCollectError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::FileRequest { failure, .. } => Some(failure),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub(crate) enum FilePlace {
    Pasted(usize),
    Discovered(FigmaFileKey),
}

impl fmt::Display for FilePlace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pasted(position) => write!(f, "Figma file list item {position}"),
            Self::Discovered(key) => write!(f, "Discovered Figma file {}", key.as_str()),
        }
    }
}

pub(crate) async fn collect_figma(
    options: &CollectOptions,
    context: &ProviderContext,
) -> Result<Vec<FigmaEntry>> {
    let IdentityEvidence::Verified(principal) = context.identity() else {
        return Err(FigmaCollectError::NoIdentity.into());
    };
    let config = context.config::<FigmaConfig>()?;
    let scope = FigmaScope::of(config)?;
    let api = FigmaApi::new(config).map_err(FigmaCollectError::Setup)?;
    let fallback = FigmaCollection::default();
    let state = context
        .collection_state::<FigmaCollection>()
        .unwrap_or(&fallback);
    let path = file_book::path(context, principal);
    let run = state.run(
        context.connection().map(|connection| &connection.id),
        path.as_deref(),
    );
    let tabs = state.tabs().await;
    collect_files(options, &api, scope, &principal.id, &run, tabs).await
}

async fn collect_files(
    options: &CollectOptions,
    api: &FigmaApi,
    scope: FigmaScope,
    principal: &str,
    run: &ConnectionRun,
    tabs: &TabSnapshot,
) -> Result<Vec<FigmaEntry>> {
    let FigmaScope {
        files,
        teams,
        duplicates,
    } = scope;
    let now = Utc::now();
    let (start, end) = options.utc_days();
    let today = now.date_naive();
    if !teams.is_empty() {
        let recent = end >= today - TimeDelta::days(1) && start <= today;
        run.team_listing
            .get_or_try_init(|| team_discovery::refresh(api, &run.book, &teams, recent))
            .await?;
    }
    let pasted = files.iter().map(|(_, file)| file.clone()).collect();
    let configured_teams = teams.into_iter().map(|(_, team)| team).collect();
    let sightings = tabs.outcome.files_for(principal).cloned().collect();
    let seen_at = tabs.outcome.has_snapshot().then_some(tabs.seen_at);
    let from_book = run
        .book
        .begin_chunk(
            sightings,
            seen_at,
            pasted,
            configured_teams,
            options.start,
            Utc::now(),
        )
        .await?;
    run.record_lost(from_book.lost);

    let files = files
        .into_iter()
        .map(|(position, key)| SelectedFile::Pasted { position, key })
        .chain(from_book.read.into_iter().map(SelectedFile::Discovered));
    // Keep the other requests polled during the identity probe and book writes.
    let mut reads = stream::iter(files.map(|file| async move {
        let result = collect_file(options, api, file.key(), principal).await;
        match (file, result) {
            (file, Ok(collected)) => Ok::<_, anyhow::Error>(Some((file, collected))),
            (SelectedFile::Discovered(file), Err(FileError::Request(error))) => {
                let error = match run.proves_loss(api, principal, &error).await {
                    Ok(true) => {
                        let observed_at = Utc::now();
                        let key = file.key.clone();
                        run.book
                            .finish_chunk(
                                vec![FileObservation {
                                    file,
                                    outcome: Observation::Lost { observed_at },
                                }],
                                observed_at,
                            )
                            .await?;
                        run.record_lost([key]);
                        return Ok(None);
                    }
                    Ok(false) => error,
                    Err(identity_error) => identity_error,
                };
                Err(request_failure(FilePlace::Discovered(file.key), error).into())
            }
            (file, Err(FileError::Request(error))) => {
                Err(request_failure(file.place(), error).into())
            }
            (file, Err(FileError::RepeatedVersions)) => {
                Err(FigmaCollectError::RepeatedVersions { file: file.place() }.into())
            }
        }
    }))
    .buffered(FILE_CONCURRENCY);
    let mut observations = Vec::new();
    let mut entries = Vec::new();
    let mut unknown_editors = 0;
    let mut rejected_links = 0;
    let mut first_other_origin = None;

    while let Some(result) = reads.next().await {
        let Some((selected, file)) = result? else {
            continue;
        };
        if let SelectedFile::Discovered(selected) = selected {
            observations.push(FileObservation {
                file: selected,
                outcome: Observation::Read {
                    observed_at: file.observed_at,
                    contributed: !file.entries.is_empty(),
                },
            });
        }
        entries.extend(file.entries);
        unknown_editors += usize::from(file.unknown_editor);
        if let Some(rejection) = file.rejected_link {
            rejected_links += 1;
            if let Unshown::Elsewhere(origin) = rejection {
                first_other_origin.get_or_insert(origin);
            }
        }
    }
    // Only successful chunks can contribute entries to the day cache. Record these
    // contributions before returning the entries for that cache to write.
    run.book.finish_chunk(observations, Utc::now()).await?;
    let source = Source::Figma.as_ref();
    if duplicates > 0 {
        diagnostics::warn(
            source,
            format!("The Figma file list contained duplicate items ({duplicates} ignored)"),
        );
    }
    if unknown_editors > 0 {
        diagnostics::warn(
            source,
            format!("Figma returned unrecognized editor types ({unknown_editors} files affected)"),
        );
    }
    if rejected_links > 0 {
        let mut message = format!(
            "Using default file links where Figma returned invalid links ({rejected_links} affected)."
        );
        if let Some(origin) = first_other_origin {
            message.push_str(&format!(
                " The first unexpected origin was {}.",
                origin.ascii_serialization()
            ));
        }
        diagnostics::warn(source, message);
    }
    Ok(entries)
}

enum SelectedFile {
    Pasted { position: usize, key: FigmaFileKey },
    Discovered(DiscoveredFile),
}

impl SelectedFile {
    fn key(&self) -> &FigmaFileKey {
        match self {
            Self::Pasted { key, .. } => key,
            Self::Discovered(file) => &file.key,
        }
    }

    fn place(&self) -> FilePlace {
        match self {
            Self::Pasted { position, .. } => FilePlace::Pasted(*position),
            Self::Discovered(file) => FilePlace::Discovered(file.key.clone()),
        }
    }
}

fn request_failure(file: FilePlace, error: ApiError) -> FigmaCollectError {
    FigmaCollectError::FileRequest {
        file,
        failure: RemoteFailure {
            service: FigmaApi::SERVICE.into(),
            error,
        },
    }
}

enum FileError {
    Request(ApiError),
    RepeatedVersions,
}

impl From<ApiError> for FileError {
    fn from(error: ApiError) -> Self {
        Self::Request(error)
    }
}

struct CollectedFile {
    observed_at: chrono::DateTime<Utc>,
    entries: Vec<FigmaEntry>,
    unknown_editor: bool,
    rejected_link: Option<Unshown>,
}

async fn collect_file(
    options: &CollectOptions,
    api: &FigmaApi,
    key: &FigmaFileKey,
    principal: &str,
) -> Result<CollectedFile, FileError> {
    let meta = api.file_meta(key).await?;
    let versions = if meta.last_touched_at.utc() >= options.start {
        api.versions_since(key, options.start)
            .await
            .map_err(|error| match error {
                VersionsError::Request(error) => FileError::Request(error),
                VersionsError::Repeated => FileError::RepeatedVersions,
            })?
    } else {
        Vec::new()
    };
    let comments = api.comments(key).await?;
    let editor = meta.editor_type.parse::<FigmaEditor>().ok();
    let (url, rejected_link) = file_link(key, meta.url.as_deref());
    let entry = |id, timestamp, activity, url: &Url| FigmaEntry {
        id,
        timestamp,
        activity,
        source: FigmaTag,
        connection: ConnectionName::unstamped(),
        file_key: key.as_str().to_owned(),
        file_name: meta.name.clone(),
        editor,
        file_url: url.as_str().to_owned(),
    };

    let mut entries: Vec<_> = versions
        .into_iter()
        .filter(|row| row.user.id == principal && options.contains(row.created_at.utc()))
        .map(|version| {
            let id = format!("{}:version:{}", key.as_str(), version.id);
            let (timestamp, activity) = version_activity(version);
            entry(id, timestamp, activity, &url)
        })
        .collect();
    let root_nodes: HashMap<_, _> = comments
        .iter()
        .filter_map(|comment| {
            if present(comment.parent_id.as_deref()).is_some() {
                return None;
            }
            present(comment.node_id()).map(|node| (comment.id.as_str(), node))
        })
        .collect();
    entries.extend(
        comments
            .iter()
            .filter(|row| row.user.id == principal && options.contains(row.created_at.utc()))
            .map(|comment| {
                let id = format!("{}:comment:{}", key.as_str(), comment.id);
                let (node, activity) = comment_activity(comment, &root_nodes);
                let link = node.map(|node| comment_link(&url, node));
                entry(
                    id,
                    comment.created_at.clone(),
                    activity,
                    link.as_ref().unwrap_or(&url),
                )
            }),
    );
    entries.sort_by(|a, b| {
        a.timestamp
            .instant()
            .cmp(&b.timestamp.instant())
            .then_with(|| a.id.cmp(&b.id))
    });
    Ok(CollectedFile {
        observed_at: Utc::now(),
        entries,
        unknown_editor: editor.is_none(),
        rejected_link,
    })
}

fn version_activity(version: FigmaVersion) -> (Timestamp, FigmaActivity) {
    let FigmaVersion {
        id,
        created_at,
        label,
        description,
        ..
    } = version;
    let activity = match label.filter(|label| present(Some(label)).is_some()) {
        Some(label) => FigmaActivity::VersionSaved {
            version_id: id,
            label,
            description: description.filter(|text| present(Some(text)).is_some()),
        },
        None => FigmaActivity::Edited { version_id: id },
    };
    (created_at, activity)
}

fn comment_activity<'a>(
    comment: &'a FigmaComment,
    root_nodes: &HashMap<&str, &'a str>,
) -> (Option<&'a str>, FigmaActivity) {
    let message = present(Some(&comment.message)).map(str::to_owned);
    if let Some(parent) = present(comment.parent_id.as_deref()) {
        let node = root_nodes.get(parent).copied();
        (
            node,
            FigmaActivity::Replied {
                comment_id: comment.id.clone(),
                parent_id: parent.to_owned(),
                message,
                node_id: node.map(str::to_owned),
            },
        )
    } else {
        let node = present(comment.node_id());
        (
            node,
            FigmaActivity::Commented {
                comment_id: comment.id.clone(),
                message,
                node_id: node.map(str::to_owned),
                resolved: comment.resolved_at.is_some(),
            },
        )
    }
}

fn present(text: Option<&str>) -> Option<&str> {
    text.filter(|text| !text.trim().is_empty())
}

fn file_link(file: &FigmaFileKey, supplied: Option<&str>) -> (Url, Option<Unshown>) {
    let mut url = Url::parse(WEB_ORIGIN).expect("Figma's fixed web origin");
    let rejected = match supplied.map(|raw| web_link(raw, &url.origin())) {
        Some(Ok(link)) => {
            let mut url = Url::parse(link.as_str()).expect("validated Figma web URL");
            url.set_query(None);
            url.set_fragment(None);
            return (url, None);
        }
        Some(Err(reason)) => Some(reason),
        None => None,
    };
    url.path_segments_mut()
        .expect("Figma's web origin has a path")
        .extend(["file", file.as_str()]);
    (url, rejected)
}

fn comment_link(file_url: &Url, node: &str) -> Url {
    let mut url = file_url.clone();
    url.query_pairs_mut().append_pair("node-id", node);
    url
}

#[cfg(test)]
mod tests;
