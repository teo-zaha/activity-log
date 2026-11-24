//! The Jira Data Center REST client used by the startup check and the collector.

use std::fmt;

use reqwest::{
    Url,
    header::{self, HeaderMap, HeaderValue},
};
use serde_json::Value;

use crate::{
    model::{RemoteIdentity, jira::jira_search_response::JiraSearchResponse},
    provider::{
        jira::JiraConfig,
        remote_api::{IdField, RemoteClient, Retry, SetupError, identity_from},
        remote_error::ApiError,
    },
};

#[derive(Debug)]
pub(crate) enum JiraResponseError {
    Remote(ApiError),
    Anonymous,
}

impl From<ApiError> for JiraResponseError {
    fn from(error: ApiError) -> Self {
        Self::Remote(error)
    }
}

impl fmt::Display for JiraResponseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Remote(error) => f.write_str(&error.message(JiraApi::SERVICE)),
            Self::Anonymous => f.write_str("Jira did not authenticate the token."),
        }
    }
}

impl std::error::Error for JiraResponseError {}

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

    async fn get_json(&self, url: Url) -> Result<Value, JiraResponseError> {
        // Check the header before decoding the endpoint's schema. An anonymous response
        // may omit fields the endpoint normally sends.
        let (value, headers) = self
            .remote
            .get_json_with_headers(url, Retry::OnceShort)
            .await?;
        if headers
            .get("x-ausername")
            .is_some_and(|user| user == "anonymous")
        {
            return Err(JiraResponseError::Anonymous);
        }
        Ok(value)
    }

    pub(crate) async fn search(&self, url: Url) -> Result<JiraSearchResponse, JiraResponseError> {
        let value = self.get_json(url).await?;
        serde_json::from_value(value).map_err(|_| ApiError::Decode.into())
    }

    /// Who the token belongs to: `/myself`'s `key` and `name`.
    pub(crate) async fn myself(&self) -> Result<Option<RemoteIdentity>, JiraResponseError> {
        let value = self.get_json(self.endpoint(&["myself"])).await?;
        identity_from(Ok(value), IdField::Text("key"), "name").map_err(JiraResponseError::from)
    }
}

#[cfg(test)]
mod tests;
