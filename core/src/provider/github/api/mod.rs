mod endpoints;

use anyhow::{Context, Result};
use reqwest::{
    Url,
    header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue},
};
use serde::{Serialize, de::DeserializeOwned};
use url::Origin;

use crate::{
    model::RemoteIdentity,
    provider::{
        http_client_builder,
        remote_api::{IdField, PostJsonError, RemoteClient, Retry, identity_from},
        remote_error::{ApiError, RemoteFailure},
    },
};

use super::GitHubConfig;
use endpoints::Endpoints;
pub(crate) use endpoints::GitHubSetupError;

pub(crate) fn web_url_problem(url: &str) -> Option<&'static str> {
    Endpoints::from_url(url).err()?.url_problem()
}

pub(crate) struct GitHubApi {
    remote: RemoteClient,
    user: Url,
    graphql: Url,
    web_origin: Origin,
}

#[derive(Serialize)]
struct Query<'a, V> {
    query: &'static str,
    variables: &'a V,
}

impl GitHubApi {
    pub(crate) const SERVICE: &str = "GitHub";

    pub(crate) fn new(config: &GitHubConfig) -> Result<Self, GitHubSetupError> {
        let endpoints = Endpoints::from_url(&config.url)?;
        let mut headers = HeaderMap::new();
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/vnd.github+json"),
        );
        headers.insert(
            "x-github-api-version",
            HeaderValue::from_static("2022-11-28"),
        );

        let remote = RemoteClient::new(
            endpoints.base.as_str(),
            AUTHORIZATION,
            &format!("Bearer {}", config.token),
            &config.token,
            http_client_builder().default_headers(headers),
        )?;
        let user = remote.endpoint(endpoints.user_path());
        let graphql = remote.endpoint(endpoints.graphql_path());

        Ok(Self {
            remote,
            user,
            graphql,
            web_origin: endpoints.web_origin,
        })
    }

    pub(crate) fn web_origin(&self) -> &Origin {
        &self.web_origin
    }

    pub(crate) async fn current_user(&self) -> Result<Option<RemoteIdentity>, ApiError> {
        identity_from(
            self.remote.get_json(self.user.clone()).await,
            IdField::Integer("id"),
            "login",
        )
    }

    pub(crate) fn failure(error: ApiError) -> RemoteFailure {
        RemoteFailure {
            service: Self::SERVICE.into(),
            error,
        }
    }

    pub(crate) async fn graphql<V: Serialize, T: DeserializeOwned>(
        &self,
        query: &'static str,
        variables: &V,
    ) -> Result<T> {
        match self
            .remote
            .post_graphql(
                self.graphql.clone(),
                &Query { query, variables },
                Retry::OnceShort,
            )
            .await
        {
            Ok(data) => Ok(data),
            Err(PostJsonError::Remote(error)) => Err(Self::failure(error).into()),
            Err(PostJsonError::Encode(error)) => {
                Err(error).context("Could not encode the GitHub query")
            }
        }
    }
}
