//! The GitLab REST client used by the startup check and the collector.

use reqwest::{Url, header::HeaderName};
use serde::de::DeserializeOwned;

use crate::{
    model::RemoteIdentity,
    provider::{
        gitlab::GitLabConfig,
        remote_api::{IdField, RemoteClient, SetupError, identity_from},
        remote_error::ApiError,
    },
};

pub(crate) struct GitLabApi {
    remote: RemoteClient,
}

impl GitLabApi {
    pub(crate) const SERVICE: &'static str = "GitLab";

    pub(crate) fn new(config: &GitLabConfig) -> Result<Self, SetupError> {
        RemoteClient::new(
            &config.url,
            HeaderName::from_static("private-token"),
            &config.token,
            &config.token,
            crate::provider::http_client_builder(),
        )
        .map(|remote| Self { remote })
    }

    /// `{base}/api/v4/{segments…}`
    pub(crate) fn endpoint(&self, segments: &[&str]) -> Url {
        let mut path = vec!["api", "v4"];
        path.extend_from_slice(segments);
        self.remote.endpoint(&path)
    }

    pub(crate) async fn get_json<T: DeserializeOwned>(&self, url: Url) -> Result<T, ApiError> {
        self.remote.get_json(url).await
    }

    /// Who the token belongs to, which may differ from the configured user whose events are
    /// collected: `/user`'s `id` and `username`.
    pub(crate) async fn current_user(&self) -> Result<Option<RemoteIdentity>, ApiError> {
        identity_from(
            self.get_json(self.endpoint(&["user"])).await,
            IdField::Integer("id"),
            "username",
        )
    }
}

#[cfg(test)]
mod tests;
