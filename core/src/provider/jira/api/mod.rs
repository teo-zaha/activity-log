//! The Jira Data Center REST client used by the startup check and the collector.

use reqwest::{
    Url,
    header::{self, HeaderMap, HeaderValue},
};
use serde::de::DeserializeOwned;

use crate::{
    model::RemoteIdentity,
    provider::{
        jira::JiraConfig,
        remote_api::{IdField, RemoteClient, SetupError, identity_from},
        remote_error::ApiError,
    },
};

pub(crate) struct JiraApi {
    remote: RemoteClient,
}

impl JiraApi {
    pub(crate) const SERVICE: &'static str = "Jira";

    pub(crate) fn new(config: &JiraConfig) -> Result<Self, SetupError> {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        let builder = crate::provider::http_client_builder()
            .default_headers(headers)
            .danger_accept_invalid_certs(config.insecure);
        RemoteClient::new(
            &config.url,
            header::AUTHORIZATION,
            &format!("Bearer {}", config.token),
            &config.token,
            builder,
        )
        .map(|remote| Self { remote })
    }

    /// `{base}/rest/api/2/{segments…}`
    pub(crate) fn endpoint(&self, segments: &[&str]) -> Url {
        let mut path = vec!["rest", "api", "2"];
        path.extend_from_slice(segments);
        self.remote.endpoint(&path)
    }

    pub(crate) async fn get_json<T: DeserializeOwned>(&self, url: Url) -> Result<T, ApiError> {
        self.remote.get_json(url).await
    }

    /// Who the token belongs to: `/myself`'s `key` and `name`.
    pub(crate) async fn myself(&self) -> Result<Option<RemoteIdentity>, ApiError> {
        identity_from(
            self.get_json(self.endpoint(&["myself"])).await,
            IdField::Text("key"),
            "name",
        )
    }
}

#[cfg(test)]
mod tests;
