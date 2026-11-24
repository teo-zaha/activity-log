//! Validates links from a provider against its web origin.

use reqwest::Url;
use url::Origin;

/// A page as a host gave it, kept only when it may be shown: http(s), with a host, on
/// the host's web origin and without user info. Made only by [`web_link`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WebLink(String);

impl WebLink {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

/// Why a link is not shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Unshown {
    /// A web page of another origin (scheme, host and port, never user info): the host's
    /// address is not the one configured, or the link leads away from it.
    Elsewhere(Origin),
    /// No http(s) address with a host, or one with user info.
    NotAWebPage,
}

/// `raw` as a link to `allowed`'s pages, or why it is not one. `Url::origin` ignores user
/// info, so it is refused on its own.
pub(crate) fn web_link(raw: &str, allowed: &Origin) -> Result<WebLink, Unshown> {
    let url = Url::parse(raw).map_err(|_| Unshown::NotAWebPage)?;
    let web_page = matches!(url.scheme(), "http" | "https")
        && url.has_host()
        && url.username().is_empty()
        && url.password().is_none();
    if !web_page {
        return Err(Unshown::NotAWebPage);
    }
    let origin = url.origin();
    if origin != *allowed {
        return Err(Unshown::Elsewhere(origin));
    }
    Ok(WebLink(url.into()))
}

#[cfg(test)]
mod tests;
