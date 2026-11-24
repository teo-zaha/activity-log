//! HTTP plumbing shared by `JiraApi` and `GitLabApi`: one configured origin that credentials
//! are only sent to, paths built from segments, and every failure through `remote_error`.

use std::time::Duration;

use chrono::{DateTime, Utc};
use reqwest::{
    Client, ClientBuilder, Response, StatusCode, Url,
    header::{HeaderMap, HeaderName, HeaderValue},
    redirect::{Action, Attempt, Policy},
};
use serde::de::DeserializeOwned;

use crate::{
    model::{IdentityEvidence, ProviderProblem, RemoteIdentity},
    provider::{
        provider_check_result::ProviderCheckResult,
        provider_runtime::check_time_left,
        remote_error::{ApiError, Quota, busy, retry_after},
    },
};

#[cfg(test)]
pub(crate) mod test_server;

const MAX_REDIRECTS: usize = 10;
/// A busy answer is retried once when the server asks to wait at most this long.
const MAX_RETRY_WAIT_SECONDS: u64 = 10;
/// How much of an error body is read, and for how long.
const ERROR_BODY_LIMIT: usize = 16 * 1024;
const ERROR_BODY_WAIT: Duration = Duration::from_secs(3);
/// Time a startup check keeps for returning a status it has already received.
const CHECK_MARGIN: Duration = Duration::from_millis(100);

/// Whether a busy answer is sent again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Retry {
    /// Once, when the server asks to wait at most `MAX_RETRY_WAIT_SECONDS`: a collection's own
    /// requests and the startup checks.
    OnceShort,
    /// Never: the busy answer is the caller's to act on at once (a change stack lookup stops
    /// all its requests on the first).
    Never,
}

/// What a paged read spends before each request it sends.
pub(crate) trait RequestBudget {
    /// Why no further request may be sent.
    type Refusal;
    fn try_spend(&self) -> Result<(), Self::Refusal>;
}

/// No limit on requests: a collection reads every page of its own list.
pub(crate) struct Unmetered;

impl RequestBudget for Unmetered {
    type Refusal = std::convert::Infallible;

    fn try_spend(&self) -> Result<(), Self::Refusal> {
        Ok(())
    }
}

/// Why a client could not be set up from a connection's values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SetupError {
    /// The URL holds a user name or password; the token belongs in the token field.
    Credentials,
    /// The URL cannot have paths below it (`mailto:…`).
    NotAnAddress,
    /// The token cannot be sent in a header.
    Token,
    Client,
}

impl SetupError {
    pub(crate) fn reason(self, service: &str) -> String {
        match self {
            Self::Credentials => {
                "Remove the user name and password from the address; the token goes in the token field.".to_string()
            }
            Self::NotAnAddress => ApiError::Unreachable.message(service),
            Self::Token => format!("{service} authentication setup failed"),
            Self::Client => format!("{service} client setup failed"),
        }
    }

    pub(crate) fn check_result(self, service: &str) -> ProviderCheckResult {
        match self {
            Self::Credentials => ProviderCheckResult::unavailable_as(
                ProviderProblem::NotConfigured,
                self.reason(service),
            ),
            _ => ProviderCheckResult::unavailable(self.reason(service)),
        }
    }
}

pub(crate) struct RemoteClient {
    base: Url,
    client: Client,
    auth_name: HeaderName,
    auth_value: HeaderValue,
    secret: String,
}

impl RemoteClient {
    /// `base` is the configured URL: trailing slashes, query and fragment are dropped, a
    /// context path (`https://host/jira`) is kept. `auth_value` is sent as `auth_name` on
    /// requests to the base's origin only; `secret` is redacted from remote messages.
    pub(crate) fn new(
        base: &str,
        auth_name: HeaderName,
        auth_value: &str,
        secret: &str,
        builder: ClientBuilder,
    ) -> Result<Self, SetupError> {
        let mut base = Url::parse(base.trim()).map_err(|_| SetupError::NotAnAddress)?;
        if base.cannot_be_a_base() {
            return Err(SetupError::NotAnAddress);
        }
        if !base.username().is_empty() || base.password().is_some() {
            return Err(SetupError::Credentials);
        }
        base.set_query(None);
        base.set_fragment(None);
        let path = base.path().trim_end_matches('/').to_string();
        base.set_path(&path);

        let mut auth_value = HeaderValue::from_str(auth_value).map_err(|_| SetupError::Token)?;
        auth_value.set_sensitive(true);
        let home = base.clone();
        let client = builder
            .redirect(Policy::custom(move |attempt| redirect(&home, attempt)))
            .build()
            .map_err(|_| SetupError::Client)?;

        Ok(Self {
            base,
            client,
            auth_name,
            auth_value,
            secret: secret.to_string(),
        })
    }

    /// The configured origin, the only one credentials are sent to.
    pub(crate) fn origin(&self) -> url::Origin {
        self.base.origin()
    }

    /// The base with `segments` appended, each one path segment whatever it holds (`/`, `?`
    /// and `#` are encoded). The URL parser drops segments that are exactly `.` or `..` and
    /// removes tabs and line breaks, so values from input pass `is_path_segment` first.
    pub(crate) fn endpoint(&self, segments: &[&str]) -> Url {
        let mut url = self.base.clone();
        url.path_segments_mut()
            .expect("`new` refuses URLs without a path")
            .pop_if_empty()
            .extend(segments);
        url
    }

    /// GETs `url` (from `endpoint`) and reads its JSON answer. `Decode` means the whole answer
    /// arrived and is not the expected JSON; a body that breaks off is a transfer failure.
    /// A busy answer is retried once ([`Retry::OnceShort`]).
    pub(crate) async fn get_json<T: DeserializeOwned>(&self, url: Url) -> Result<T, ApiError> {
        self.get_json_with_headers(url, Retry::OnceShort)
            .await
            .map(|(value, _)| value)
    }

    /// [`Self::get_json`], with the answer's headers (a paged API names its next page there),
    /// retrying a busy answer as `retry` says.
    pub(crate) async fn get_json_with_headers<T: DeserializeOwned>(
        &self,
        url: Url,
        retry: Retry,
    ) -> Result<(T, reqwest::header::HeaderMap), ApiError> {
        // Only `endpoint` builds URLs, so this holds; it keeps the credentials at home if a
        // caller ever changes that.
        if url.origin() != self.base.origin() {
            return Err(ApiError::Unreachable);
        }

        let response = self.send(&url).await?;
        let received_at = Utc::now();
        let budget = check_time_left().map(|left| left.saturating_sub(CHECK_MARGIN));
        if retry == Retry::OnceShort
            && let Some(wait) = retry_wait(response.status(), response.headers(), received_at)
                .map(Duration::from_secs)
            && budget.is_none_or(|left| wait < left)
        {
            let fallback = budget.map(|left| {
                let error = ApiError::from_status(
                    response.status(),
                    response.headers(),
                    received_at,
                    "",
                    &[&self.secret],
                );
                (left, error)
            });
            drop(response);
            let retried = async {
                tokio::time::sleep(wait).await;
                let response = self.send(&url).await?;
                self.read_response(response, Utc::now()).await
            };
            return match fallback {
                Some((left, busy)) => tokio::time::timeout(left, retried)
                    .await
                    .unwrap_or(Err(busy)),
                None => retried.await,
            };
        }
        self.read_response(response, received_at).await
    }

    async fn send(&self, url: &Url) -> Result<Response, ApiError> {
        self.client
            .get(url.clone())
            .header(&self.auth_name, &self.auth_value)
            .send()
            .await
            .map_err(|err| ApiError::from_transport(&err))
    }

    async fn read_response<T: DeserializeOwned>(
        &self,
        response: Response,
        received_at: DateTime<Utc>,
    ) -> Result<(T, HeaderMap), ApiError> {
        let status = response.status();
        let headers = response.headers().clone();
        if !status.is_success() {
            let body = error_body(response).await;
            return Err(ApiError::from_status(
                status,
                &headers,
                received_at,
                &body,
                &[&self.secret],
            ));
        }
        let body = response
            .bytes()
            .await
            .map_err(|err| ApiError::from_transport(&err))?;
        let value = serde_json::from_slice(&body).map_err(|_| ApiError::Decode)?;
        Ok((value, headers))
    }
}

/// `value` reaches the server as itself in one path segment.
pub(crate) fn is_path_segment(value: &str) -> bool {
    !value.is_empty() && !matches!(value, "." | "..") && !value.chars().any(char::is_control)
}

/// The start of an error body, within `ERROR_BODY_LIMIT` and `ERROR_BODY_WAIT`; empty when it
/// does not arrive in time or is longer, since a body cut at the limit explains nothing.
/// Inside a startup check the read also ends `CHECK_MARGIN` before the check's deadline, so
/// the check can report a status it already received.
async fn error_body(response: Response) -> String {
    let wait = match check_time_left() {
        Some(left) => ERROR_BODY_WAIT.min(left.saturating_sub(CHECK_MARGIN)),
        None => ERROR_BODY_WAIT,
    };
    match tokio::time::timeout(wait, read_prefix(response, ERROR_BODY_LIMIT)).await {
        Ok((body, true)) => String::from_utf8_lossy(&body).into_owned(),
        _ => String::new(),
    }
}

/// At most `limit` bytes of the body, and whether that is all of it. A body that breaks off
/// gives nothing.
async fn read_prefix(mut response: Response, limit: usize) -> (Vec<u8>, bool) {
    let mut body = Vec::new();
    while body.len() < limit {
        match response.chunk().await {
            Ok(Some(chunk)) => {
                let room = limit - body.len();
                body.extend_from_slice(&chunk[..chunk.len().min(room)]);
                if chunk.len() > room {
                    return (body, false);
                }
            }
            Ok(None) => return (body, true),
            Err(_) => return (Vec::new(), false),
        }
    }
    (body, false)
}

/// A redirect is followed only to the configured origin: same scheme, host and port. reqwest
/// strips `Authorization` on such a change, but never `PRIVATE-TOKEN`.
fn redirect(base: &Url, attempt: Attempt) -> Action {
    if attempt.previous().len() > MAX_REDIRECTS {
        attempt.error("too many redirects")
    } else if follows(base, attempt.url()) {
        attempt.follow()
    } else {
        attempt.stop()
    }
}

fn follows(base: &Url, next: &Url) -> bool {
    next.origin() == base.origin()
}

/// How long to wait before the one retry of a busy answer, if it gets one.
fn retry_wait(status: StatusCode, headers: &HeaderMap, now: DateTime<Utc>) -> Option<u64> {
    let wait = retry_after(headers, now)?;
    (busy(status, Some(wait), Quota::from_headers(headers)) && wait <= MAX_RETRY_WAIT_SECONDS)
        .then_some(wait)
}

/// A startup check's answer from its identity request (Jira `/myself`, GitLab `/user`):
/// `Ok(None)` is a complete success that named no one. Status and headers tell a refusal
/// from a busy answer. A complete success that is not the API's JSON (an SSO sign-in page)
/// leaves the connection unavailable and names no one.
pub(crate) fn identity_check(
    service: &str,
    answer: Result<Option<RemoteIdentity>, ApiError>,
) -> ProviderCheckResult {
    match answer {
        Ok(Some(identity)) => {
            ProviderCheckResult::available().with_identity(IdentityEvidence::Verified(identity))
        }
        Ok(None) => ProviderCheckResult::available().with_identity(IdentityEvidence::Unnamed),
        Err(err) => {
            let problem = err.problem().unwrap_or(ProviderProblem::Failed);
            match err {
                ApiError::Decode => ProviderCheckResult::unavailable(format!(
                    "{service} answered with a page that is not its API (a sign-in page?); check the URL and token"
                )),
                ApiError::Status {
                    status, message, ..
                } if problem == ProviderProblem::Refused => {
                    let short = if status == StatusCode::UNAUTHORIZED {
                        format!("{service} credentials invalid")
                    } else {
                        format!("{service} token lacks access")
                    };
                    ProviderCheckResult::unavailable_as(
                        problem,
                        if explains(status, &message) {
                            format!("{short}: {message}")
                        } else {
                            short
                        },
                    )
                    .with_identity(IdentityEvidence::Denied)
                }
                err => ProviderCheckResult::unavailable_as(problem, err.message(service)),
            }
        }
    }
}

/// `message` says more than the status line (`401 Unauthorized`, `Unauthorized`).
fn explains(status: StatusCode, message: &str) -> bool {
    let message = message.trim();
    let reason = status.canonical_reason().unwrap_or_default();
    !(message.is_empty()
        || message.eq_ignore_ascii_case(reason)
        || message.eq_ignore_ascii_case(&status.to_string()))
}

/// Where an identity answer names its principal, in the type the server declares.
#[derive(Debug, Clone, Copy)]
pub(crate) enum IdField {
    /// A non-empty string (Jira's `key`).
    Text(&'static str),
    /// A non-negative whole number (GitLab's `id`).
    Integer(&'static str),
}

/// An identity request's answer: an id of the declared type names the principal; a success
/// without one names no one. Failures pass through, `Decode` included.
pub(crate) fn identity_from(
    answer: Result<serde_json::Value, ApiError>,
    id_field: IdField,
    username_field: &str,
) -> Result<Option<RemoteIdentity>, ApiError> {
    let value = answer?;
    let id = match id_field {
        IdField::Text(field) => value
            .get(field)
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(str::to_string),
        IdField::Integer(field) => value
            .get(field)
            .and_then(serde_json::Value::as_u64)
            .map(|id| id.to_string()),
    };
    let Some(id) = id else {
        return Ok(None);
    };
    let username = value
        .get(username_field)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_string();
    Ok(Some(RemoteIdentity { id, username }))
}

#[cfg(test)]
mod tests;
