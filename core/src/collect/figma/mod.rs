use std::{collections::HashMap, fmt};

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

const FILE_CONCURRENCY: usize = 3;
const WEB_ORIGIN: &str = "https://www.figma.com";

#[derive(Debug)]
pub(crate) enum FigmaCollectError {
    NoIdentity,
    Setup(SetupError),
    FileRequest {
        position: usize,
        failure: RemoteFailure,
    },
    RepeatedVersions {
        position: usize,
    },
}

impl fmt::Display for FigmaCollectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoIdentity => f.write_str(
                "Figma did not say whose token this is. Activity Log cannot identify your versions or comments.",
            ),
            Self::Setup(error) => f.write_str(&error.reason(FigmaApi::SERVICE)),
            Self::FileRequest { position, .. } => write!(f, "Could not read Figma file list item {position}"),
            Self::RepeatedVersions { position } => {
                write!(f, "Figma repeated version history for file list item {position}")
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
    collect_files(options, &api, scope, &principal.id).await
}

async fn collect_files(
    options: &CollectOptions,
    api: &FigmaApi,
    scope: FigmaScope,
    principal: &str,
) -> Result<Vec<FigmaEntry>> {
    let FigmaScope { files, duplicates } = scope;
    let mut reads = stream::iter(files.into_iter().map(|(position, file)| async move {
        (position, collect_file(options, api, &file, principal).await)
    }))
    .buffered(FILE_CONCURRENCY);
    let mut entries = Vec::new();
    let mut unknown_editors = 0;
    let mut rejected_links = 0;
    let mut first_other_origin = None;

    while let Some((position, result)) = reads.next().await {
        match result {
            Ok(file) => {
                entries.extend(file.entries);
                unknown_editors += usize::from(file.unknown_editor);
                if let Some(rejection) = file.rejected_link {
                    rejected_links += 1;
                    if let Unshown::Elsewhere(origin) = rejection {
                        first_other_origin.get_or_insert(origin);
                    }
                }
            }
            Err(FileError::Request(error)) => {
                return Err(FigmaCollectError::FileRequest {
                    position,
                    failure: RemoteFailure {
                        service: FigmaApi::SERVICE.into(),
                        error,
                    },
                }
                .into());
            }
            Err(FileError::RepeatedVersions) => {
                return Err(FigmaCollectError::RepeatedVersions { position }.into());
            }
        }
    }
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
