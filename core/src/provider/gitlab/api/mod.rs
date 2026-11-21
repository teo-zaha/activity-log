//! The GitLab REST client used by the startup check, the collector and the change stack
//! lookup.

mod merge_requests;
mod pager;

pub(crate) use merge_requests::{MergeRequestFilter, MergeRequestRow};
pub(crate) use pager::{PageError, PagedRow, Pager};

use reqwest::{Url, header::HeaderName};
use serde::de::DeserializeOwned;
use url::Origin;

use crate::{
    model::RemoteIdentity,
    provider::{
        gitlab::GitLabConfig,
        remote_api::{IdField, RemoteClient, Retry, SetupError, identity_from},
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

    /// Where its pages are: the configured URL's origin, which links to merge requests must
    /// point to.
    pub(crate) fn web_origin(&self) -> Origin {
        self.remote.origin()
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

    /// One page of a paged list, and the URL of the next as GitLab names it: `x-next-page`
    /// (empty on the last page), else a `Link` header's `rel="next"`; `None` when there is none.
    /// Lists are read through [`Pager`].
    async fn get_page<T: DeserializeOwned>(
        &self,
        url: Url,
        retry: Retry,
    ) -> Result<(T, Option<Url>), ApiError> {
        let (page, headers) = self
            .remote
            .get_json_with_headers(url.clone(), retry)
            .await?;
        Ok((page, next_page(&url, &headers)))
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

/// The next page of `url` by GitLab's pagination headers.
fn next_page(url: &Url, headers: &reqwest::header::HeaderMap) -> Option<Url> {
    let text = |name: &str| headers.get(name).and_then(|value| value.to_str().ok());
    if let Some(next) = text("x-next-page") {
        let next = next.trim().parse::<u64>().ok()?;
        let mut url = url.clone();
        let pairs = url
            .query_pairs()
            .filter(|(name, _)| name != "page")
            .map(|(name, value)| (name.into_owned(), value.into_owned()))
            .collect::<Vec<_>>();
        url.query_pairs_mut()
            .clear()
            .extend_pairs(pairs)
            .append_pair("page", &next.to_string());
        return Some(url);
    }
    // A server may send several `Link` header lines; each may list several links.
    headers
        .get_all("link")
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .find_map(|link| {
            let (target, params) = link.split_once(';')?;
            params
                .split(';')
                .any(|param| param.trim() == r#"rel="next""#)
                .then(|| {
                    Url::parse(target.trim().trim_start_matches('<').trim_end_matches('>')).ok()
                })
                .flatten()
        })
}

#[cfg(test)]
mod tests;
