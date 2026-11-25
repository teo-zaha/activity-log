//! The Figma REST calls used by the check and collector. Response shapes use Figma's
//! OpenAPI spec (`figma/rest-api-spec` v0.47.0) as their reference.

use chrono::{DateTime, Utc};
use reqwest::{
    Url,
    header::{HeaderName, LINK},
};
use serde::de::DeserializeOwned;
use serde_json::Value;

use super::{
    config::FigmaConfig,
    scope::{FigmaFileKey, FigmaTeamId},
};
use crate::{
    model::{
        RemoteIdentity,
        figma::{
            FigmaComment, FigmaCommentsPage, FigmaFileMeta, FigmaFileMetaResponse, FigmaFolder,
            FigmaFolderFiles, FigmaFolderId, FigmaFolders, FigmaVersion, FigmaVersionsPage,
        },
    },
    provider::{
        http_client_builder,
        remote_api::{IdField, RemoteClient, Retry, SetupError, identity_from, paging::PageGuard},
        remote_error::ApiError,
    },
};

pub(crate) struct FigmaApi {
    remote: RemoteClient,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum VersionsError {
    Request(ApiError),
    Repeated,
}

pub(crate) enum FolderListingError {
    Request(ApiError),
    Paginated,
}

impl FigmaApi {
    pub(crate) const SERVICE: &str = "Figma";
    const ORIGIN: &str = "https://api.figma.com";

    pub(crate) fn new(config: &FigmaConfig) -> Result<Self, SetupError> {
        Self::with_base(Self::ORIGIN, config)
    }

    fn with_base(base: &str, config: &FigmaConfig) -> Result<Self, SetupError> {
        RemoteClient::new(
            base,
            HeaderName::from_static("x-figma-token"),
            &config.token,
            &config.token,
            http_client_builder(),
        )
        .map(|remote| Self { remote })
    }

    #[cfg(test)]
    pub(crate) fn at(base: &str, config: &FigmaConfig) -> Result<Self, SetupError> {
        Self::with_base(base, config)
    }

    fn file_endpoint(&self, file: &FigmaFileKey, operation: &str) -> Url {
        self.remote
            .endpoint(&["v1", "files", file.as_str(), operation])
    }

    pub(crate) async fn me(&self) -> Result<Option<RemoteIdentity>, ApiError> {
        let url = self.remote.endpoint(&["v1", "me"]);
        identity_from(
            self.remote.get_json(url).await,
            IdField::Text("id"),
            "handle",
        )
    }

    pub(crate) async fn team_folders(
        &self,
        team: &FigmaTeamId,
    ) -> Result<Vec<FigmaFolder>, FolderListingError> {
        self.folders("teams", team.as_str()).await
    }

    pub(crate) async fn subfolders(
        &self,
        folder: &FigmaFolderId,
    ) -> Result<Vec<FigmaFolder>, FolderListingError> {
        self.folders("folders", folder.as_str()).await
    }

    async fn folders(&self, route: &str, id: &str) -> Result<Vec<FigmaFolder>, FolderListingError> {
        let url = self.remote.endpoint(&["v2", route, id, "folders"]);
        let response: FigmaFolders = self.complete_folder_listing(url).await?;
        Ok(response.folders)
    }

    pub(crate) async fn folder_files(
        &self,
        folder: &FigmaFolderId,
    ) -> Result<FigmaFolderFiles, FolderListingError> {
        let url = self
            .remote
            .endpoint(&["v2", "folders", folder.as_str(), "files"]);
        self.complete_folder_listing(url).await
    }

    async fn complete_folder_listing<T: DeserializeOwned>(
        &self,
        url: Url,
    ) -> Result<T, FolderListingError> {
        let (answer, headers): (Value, _) = self
            .remote
            .get_json_with_headers(url, Retry::OnceShort)
            .await
            .map_err(FolderListingError::Request)?;
        let continuation = |page: &Value| {
            [
                "cursor",
                "next_cursor",
                "next_page",
                "nextCursor",
                "nextPage",
            ]
            .into_iter()
            .any(|field| page.get(field).is_some_and(|value| !value.is_null()))
        };
        let next_header = ["x-next-page", "x-next-cursor"].into_iter().any(|name| {
            headers
                .get_all(name)
                .iter()
                .any(|value| !value.as_bytes().is_empty())
        });
        let next_link = headers.get_all(LINK).iter().any(|link| {
            link.to_str().is_ok_and(|link| {
                link.split(';').skip(1).any(|parameter| {
                    let relation = parameter.split(',').next().unwrap_or_default().trim();
                    relation.split_once('=').is_some_and(|(name, value)| {
                        name.trim().eq_ignore_ascii_case("rel")
                            && value
                                .trim()
                                .trim_matches('"')
                                .split_ascii_whitespace()
                                .any(|kind| kind.eq_ignore_ascii_case("next"))
                    })
                })
            })
        });
        if continuation(&answer) || continuation(&answer["pagination"]) || next_header || next_link
        {
            return Err(FolderListingError::Paginated);
        }
        serde_json::from_value(answer).map_err(|_| FolderListingError::Request(ApiError::Decode))
    }

    pub(crate) async fn file_meta(&self, file: &FigmaFileKey) -> Result<FigmaFileMeta, ApiError> {
        let response: FigmaFileMetaResponse = self
            .remote
            .get_json(self.file_endpoint(file, "meta"))
            .await?;
        Ok(response.file)
    }

    pub(crate) async fn comments(
        &self,
        file: &FigmaFileKey,
    ) -> Result<Vec<FigmaComment>, ApiError> {
        let response: FigmaCommentsPage = self
            .remote
            .get_json(self.file_endpoint(file, "comments"))
            .await?;
        Ok(response.comments)
    }

    /// Reads versions back to `start`. The next-page URL is only a signal to continue.
    pub(crate) async fn versions_since(
        &self,
        file: &FigmaFileKey,
        start: DateTime<Utc>,
    ) -> Result<Vec<FigmaVersion>, VersionsError> {
        let mut guard = PageGuard::new();
        let mut before = None;
        let mut versions = Vec::new();
        loop {
            guard
                .begin(before.clone())
                .map_err(|_| VersionsError::Repeated)?;
            let page = self
                .version_page(file, before.as_deref())
                .await
                .map_err(VersionsError::Request)?;
            let Some(oldest) = page
                .versions
                .iter()
                .rev()
                .min_by_key(|row| row.created_at.utc())
            else {
                return Ok(versions);
            };
            let last = oldest.created_at.utc() < start || page.pagination.next_page.is_none();
            before = Some(oldest.id.clone());
            versions.extend(
                guard
                    .new_rows(page.versions, |row| row.id.clone())
                    .map_err(|_| VersionsError::Repeated)?
                    .into_iter()
                    .filter(|row| row.created_at.utc() >= start),
            );
            if last {
                return Ok(versions);
            }
        }
    }

    async fn version_page(
        &self,
        file: &FigmaFileKey,
        before: Option<&str>,
    ) -> Result<FigmaVersionsPage, ApiError> {
        let mut url = self.file_endpoint(file, "versions");
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("page_size", "50");
            if let Some(before) = before {
                query.append_pair("before", before);
            }
        }
        self.remote.get_json(url).await
    }
}

#[cfg(test)]
mod tests;
