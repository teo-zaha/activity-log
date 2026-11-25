pub(crate) mod api;
mod config;

pub use config::*;

use crate::{
    collect::jira::collect_jira,
    model::{IntoActivityEntries, ProviderProblem, Source, SupportedOs},
    provider::{provider_check_result::ProviderCheckResult, remote_api::identity_check},
};
use api::{JiraApi, JiraResponseError};

crate::provider! {
    JiraProvider(Source::Jira, &[SupportedOs::Windows, SupportedOs::Macos, SupportedOs::Linux], config: JiraConfig, cache: crate::cache::CachePolicy::api().without_run_chunks()) {
        async fn startup_check(&self, _context, config) {
            let api = match JiraApi::new(config) {
                Ok(api) => api,
                Err(err) => return err.check_result(JiraApi::SERVICE),
            };
            match api.myself().await {
                Ok(Some(identity)) => identity_check(JiraApi::SERVICE, Ok(Some(identity))),
                Ok(None) => ProviderCheckResult::unavailable_as(
                    ProviderProblem::Refused,
                    "Jira did not identify an account for the token.",
                ),
                Err(error @ JiraResponseError::Anonymous) => {
                    ProviderCheckResult::unavailable_as(ProviderProblem::Refused, error.to_string())
                }
                Err(JiraResponseError::Remote(error)) => identity_check(JiraApi::SERVICE, Err(error)),
            }
        }

        async fn collect(&self, options, context) {
            collect_jira(&options, &context)
                .await
                .map(|entries| entries.into_activity_entries())
        }

        fn credential_setup(&self) -> Option<&'static crate::provider::CredentialSetup> {
            Some(&config::CREDENTIAL_SETUP)
        }

        fn description(&self) -> &'static str {
            "Issues assigned to or reported by you"
        }

        fn instances(&self) -> crate::provider::connection::Instances {
            crate::provider::connection::Instances::Multiple
        }
    }
}
