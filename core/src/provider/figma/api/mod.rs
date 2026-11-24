//! The Figma REST calls used by the check and collector. Response shapes use Figma's
//! OpenAPI spec (`figma/rest-api-spec` v0.47.0) as their reference.

use reqwest::{Url, header::HeaderName};

use super::{config::FigmaConfig, scope::FigmaFileKey};
use crate::{
    model::{
        RemoteIdentity,
        figma::{
            FigmaComment, FigmaCommentsPage, FigmaFileMeta, FigmaFileMetaResponse, FigmaVersion,
            FigmaVersionsPage,
        },
    },
    provider::{
        http_client_builder,
        remote_api::{IdField, RemoteClient, SetupError, identity_from},
        remote_error::ApiError,
    },
};

pub(crate) struct FigmaApi {
    remote: RemoteClient,
}

impl FigmaApi {
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

    async fn version_page(
        &self,
        file: &FigmaFileKey,
        before: Option<&FigmaVersion>,
    ) -> Result<FigmaVersionsPage, ApiError> {
        let mut url = self.file_endpoint(file, "versions");
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("page_size", "50");
            if let Some(version) = before {
                query.append_pair("before", &version.id);
            }
        }
        self.remote.get_json(url).await
    }
}

#[cfg(test)]
mod tests;
