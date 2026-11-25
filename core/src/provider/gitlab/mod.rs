mod annotation;
pub(crate) mod api;
mod config;
mod stack;

pub use config::*;

use reqwest::StatusCode;

use crate::{
    collect::{change_stack::FailureKind, gitlab::collect_gitlab},
    model::{IntoActivityEntries, Source, SupportedOs},
    provider::{
        remote_api::identity_check,
        remote_error::{ApiError, OAuthRefusal},
    },
};
use api::GitLabApi;

const ANNOTATION_CONCURRENCY: usize = 4;

crate::provider! {
    GitLabProvider(Source::Gitlab, &[SupportedOs::Windows, SupportedOs::Macos, SupportedOs::Linux], config: GitLabConfig, cache: crate::cache::CachePolicy::api()) {
        async fn startup_check(&self, _context, config) {
            let api = match GitLabApi::new(config) {
                Ok(api) => api,
                Err(err) => return err.check_result(GitLabApi::SERVICE),
            };
            identity_check(GitLabApi::SERVICE, api.current_user().await)
        }

        async fn collect(&self, options, context) {
            collect_gitlab(&options, &context)
                .await
                .map(|entries| entries.into_activity_entries())
        }

        fn credential_setup(&self) -> Option<&'static crate::provider::CredentialSetup> {
            Some(&config::CREDENTIAL_SETUP)
        }

        fn description(&self) -> &'static str {
            "Merge requests, pushes, comments and reviews"
        }

        fn instances(&self) -> crate::provider::connection::Instances {
            crate::provider::connection::Instances::Multiple
        }

        fn annotate<'a>(
            &'a self,
            entries: &'a mut [crate::model::ActivityEntry],
            context: crate::provider::provider_context::ProviderContext,
        ) -> futures::future::BoxFuture<'a, ()> {
            Box::pin(annotation::annotate(entries, context))
        }
    }
}

/// After the busy check, an OAuth code takes precedence over the status. GitLab answers a
/// project hidden from the token with 404, and merge requests it may not read with 403.
fn lookup_failure_kind(error: &ApiError) -> FailureKind {
    if error.is_busy() {
        return FailureKind::Throttled;
    }
    match error {
        ApiError::Status {
            refusal: Some(refusal),
            ..
        } => match refusal {
            OAuthRefusal::InvalidToken => FailureKind::TokenInvalid,
            OAuthRefusal::InsufficientScope => FailureKind::ScopeMissing,
            OAuthRefusal::InvalidRequest => FailureKind::Failed,
        },
        ApiError::Status {
            status,
            refusal: None,
            ..
        } => match *status {
            StatusCode::UNAUTHORIZED => FailureKind::TokenInvalid,
            StatusCode::FORBIDDEN | StatusCode::NOT_FOUND => FailureKind::Unreadable,
            _ => FailureKind::Failed,
        },
        ApiError::Query(_) | ApiError::Timeout | ApiError::Unreachable | ApiError::Decode => {
            FailureKind::Failed
        }
    }
}
