//! Where a feed is read from, and reading it.

use std::path::PathBuf;

use anyhow::{Context, Result, anyhow, bail};
use reqwest::Url;

use crate::provider::remote_error::{ApiError, RemoteFailure};

#[cfg(test)]
mod tests;

/// Where a feed is read from.
#[derive(Clone, PartialEq, Eq)]
pub(crate) enum FeedLocation {
    Url(Url),
    File(PathBuf),
}

impl std::fmt::Debug for FeedLocation {
    // A feed's link grants access to the calendar: only its host.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Url(url) => write!(f, "Url({})", url.host_str().unwrap_or_default()),
            Self::File(path) => f.debug_tuple("File").field(path).finish(),
        }
    }
}

/// `https://…`, `webcal://…` (read over HTTPS), an absolute path or a `file://` URL. Plain
/// `http://` is refused: the link works like a password.
pub(crate) fn feed_location(raw: &str) -> Result<FeedLocation> {
    let raw = raw.trim();
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        return Ok(FeedLocation::File(path));
    }
    let raw = match raw.get(..9) {
        Some(scheme) if scheme.eq_ignore_ascii_case("webcal://") => {
            format!("https://{}", &raw[9..])
        }
        _ => raw.to_string(),
    };
    let url = Url::parse(&raw).map_err(|_| {
        anyhow!("Expected an https:// or webcal:// link, or the path of an .ics file")
    })?;
    match url.scheme() {
        "https" => Ok(FeedLocation::Url(url)),
        "file" => url
            .to_file_path()
            .map(FeedLocation::File)
            .map_err(|()| anyhow!("Not a file path")),
        "http" => bail!("Use an https:// link; calendar links are secrets"),
        other => bail!("Unsupported link type {other}://"),
    }
}

/// The feed's text. Errors never contain the link, which grants access to the calendar.
pub async fn read_feed(raw: &str) -> Result<String> {
    match feed_location(raw)? {
        // A local export is small; reading it blocks only briefly.
        FeedLocation::File(path) => {
            std::fs::read_to_string(&path).with_context(|| format!("Reading {}", path.display()))
        }
        FeedLocation::Url(url) => {
            let (host, response) = request_feed(url).await?;
            response.text().await.map_err(|err| {
                anyhow::Error::new(RemoteFailure {
                    service: host.into(),
                    error: ApiError::from_transport(&err),
                })
            })
        }
    }
}

/// Whether the feed can be read, without reading it: the collection that follows downloads it,
/// and a feed is often large. A link answers with a success status; a file can be opened.
pub async fn check_feed(raw: &str) -> Result<()> {
    match feed_location(raw)? {
        FeedLocation::File(path) => {
            let metadata = std::fs::File::open(&path)
                .and_then(|file| file.metadata())
                .with_context(|| format!("Reading {}", path.display()))?;
            if !metadata.is_file() {
                bail!("{} is no file", path.display());
            }
            Ok(())
        }
        FeedLocation::Url(url) => request_feed(url).await.map(drop),
    }
}

/// The answer to a feed's link once its status is known, before its body is read. A failure
/// keeps how it failed (`RemoteFailure`: unreachable, timed out, refused or another status),
/// named by the host, never by the link.
async fn request_feed(url: Url) -> Result<(String, reqwest::Response)> {
    let host = url.host_str().unwrap_or("the calendar host").to_string();
    let failed = |error| RemoteFailure {
        service: host.clone().into(),
        error,
    };
    let client = crate::provider::http_client_builder()
        .build()
        .map_err(|err| anyhow!("HTTP client setup failed: {}", err.without_url()))?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|err| failed(ApiError::from_transport(&err)))?;
    let status = response.status();
    if !status.is_success() {
        return Err(failed(ApiError::Status {
            status,
            message: String::new(),
            retry_after: None,
            refusal: None,
        })
        .into());
    }
    Ok((host, response))
}
