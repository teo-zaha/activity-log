use reqwest::Url;
use url::Origin;

use crate::{
    model::ProviderProblem,
    provider::{provider_check_result::ProviderCheckResult, remote_api::SetupError},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GitHubSetupError {
    Remote(SetupError),
    CloudOrigin,
    ServerApiPath,
}

impl From<SetupError> for GitHubSetupError {
    fn from(error: SetupError) -> Self {
        Self::Remote(error)
    }
}

impl GitHubSetupError {
    const CLOUD_ORIGIN: &str = "Use the GitHub web URL with https and the default port.";
    const SERVER_API_PATH: &str = "Enter the GitHub web URL without /api/v3 or /api/graphql.";

    pub(super) fn url_problem(self) -> Option<&'static str> {
        match self {
            Self::CloudOrigin => Some(Self::CLOUD_ORIGIN),
            Self::ServerApiPath => Some(Self::SERVER_API_PATH),
            Self::Remote(_) => None,
        }
    }

    pub(crate) fn reason(self, service: &str) -> String {
        match self {
            Self::Remote(error) => error.reason(service),
            Self::CloudOrigin => Self::CLOUD_ORIGIN.to_owned(),
            Self::ServerApiPath => Self::SERVER_API_PATH.to_owned(),
        }
    }

    pub(crate) fn check_result(self, service: &str) -> ProviderCheckResult {
        match self {
            Self::Remote(error) => error.check_result(service),
            _ => ProviderCheckResult::unavailable_as(
                ProviderProblem::NotConfigured,
                self.reason(service),
            ),
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum Installation {
    Cloud,
    Server,
}

pub(super) struct Endpoints {
    pub(super) base: Url,
    pub(super) web_origin: Origin,
    pub(super) installation: Installation,
}

impl Endpoints {
    pub(super) fn from_url(configured: &str) -> Result<Self, GitHubSetupError> {
        let mut base = Url::parse(configured.trim()).map_err(|_| SetupError::NotAnAddress)?;
        if !base.username().is_empty() || base.password().is_some() {
            return Err(SetupError::Credentials.into());
        }
        let host = base.host_str().ok_or(SetupError::NotAnAddress)?;
        if !matches!(base.scheme(), "http" | "https") {
            return Err(SetupError::NotAnAddress.into());
        }

        let host = host.strip_suffix('.').unwrap_or(host);
        let cloud_host = if host == "github.com" {
            Some("api.github.com".to_owned())
        } else {
            host.strip_suffix(".ghe.com")
                .filter(|tenant| !tenant.is_empty() && !tenant.contains('.'))
                .map(|_| format!("api.{host}"))
        };
        let web_origin = base.origin();
        let installation = if let Some(host) = cloud_host {
            // Cloud endpoints are HTTPS on the standard port. The entered port belongs
            // to the web origin that the token is sealed for.
            if base.scheme() != "https" || base.port().is_some() {
                return Err(GitHubSetupError::CloudOrigin);
            }
            base.set_host(Some(&host))
                .map_err(|_| SetupError::NotAnAddress)?;
            base.set_path("/");
            Installation::Cloud
        } else {
            let path = base.path().trim_end_matches('/');
            if path.ends_with("/api/v3") || path.ends_with("/api/graphql") {
                return Err(GitHubSetupError::ServerApiPath);
            }
            Installation::Server
        };

        Ok(Self {
            base,
            web_origin,
            installation,
        })
    }

    pub(super) fn user_path(&self) -> &[&str] {
        match self.installation {
            Installation::Cloud => &["user"],
            Installation::Server => &["api", "v3", "user"],
        }
    }

    pub(super) fn graphql_path(&self) -> &[&str] {
        match self.installation {
            Installation::Cloud => &["graphql"],
            Installation::Server => &["api", "graphql"],
        }
    }
}

#[cfg(test)]
mod tests;
