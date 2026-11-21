//! Which links of a change may be shown: only to its host's web pages.

use reqwest::Url;
use url::Origin;

/// A change's page as a host gave it, kept only when it may be shown: http(s), with a host, on
/// the host's web origin and without user info. Made only by [`web_link`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WebLink(String);

impl WebLink {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

/// `raw` as a link to `allowed`'s pages, or `None`. `Url::origin` ignores user info, so it is
/// refused on its own.
pub(crate) fn web_link(raw: &str, allowed: &Origin) -> Option<WebLink> {
    let url = Url::parse(raw).ok()?;
    let shown = matches!(url.scheme(), "http" | "https")
        && url.has_host()
        && url.username().is_empty()
        && url.password().is_none()
        && url.origin() == *allowed;
    shown.then(|| WebLink(url.into()))
}

#[cfg(test)]
mod tests;
