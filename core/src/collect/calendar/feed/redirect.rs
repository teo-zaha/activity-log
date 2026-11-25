use std::{error::Error, fmt};

use reqwest::redirect::Policy;

/// A refused HTTPS downgrade. Keep only the original feed host for diagnostics.
#[derive(Debug, Clone)]
pub(super) struct InsecureRedirect {
    host: String,
}

impl InsecureRedirect {
    /// Recover the typed, safe cause without retaining reqwest's URLs or error chain.
    pub(super) fn from_transport(error: &reqwest::Error) -> Option<Self> {
        let mut source = error.source();
        while let Some(cause) = source {
            if let Some(refusal) = cause.downcast_ref::<Self>() {
                return Some(refusal.clone());
            }
            source = cause.source();
        }
        None
    }
}

impl fmt::Display for InsecureRedirect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "The calendar server at {} redirected to an insecure (http) address. Activity Log refused it.",
            self.host
        )
    }
}

impl Error for InsecureRedirect {}

pub(super) fn policy(host: String) -> Policy {
    Policy::custom(move |attempt| {
        if attempt.url().scheme() == "http"
            && attempt
                .previous()
                .last()
                .is_some_and(|url| url.scheme() == "https")
        {
            attempt.error(InsecureRedirect { host: host.clone() })
        } else {
            Policy::default().redirect(attempt)
        }
    })
}
