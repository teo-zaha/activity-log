pub(crate) mod api;
mod config;

pub use config::*;

use crate::{
    collect::jira::collect_jira,
    model::{IntoActivityEntries, Source, SupportedOs},
    provider::{provider_check_result::ProviderCheckResult, remote_api::identity_check},
};
use api::JiraApi;

crate::provider! {
    JiraProvider(Source::Jira, &[SupportedOs::Windows, SupportedOs::Macos, SupportedOs::Linux], JiraConfig, cache: crate::cache::CachePolicy::api()) {
        async fn startup_check(&self, _current_os, context) {
            let config = match context.config::<JiraConfig>() {
                Ok(c) => c,
                Err(_) => {
                    return ProviderCheckResult::unavailable("Jira config resolution failed");
                }
            };
            let api = match JiraApi::new(config) {
                Ok(api) => api,
                Err(err) => return err.check_result(JiraApi::SERVICE),
            };
            identity_check(JiraApi::SERVICE, api.myself().await)
        }

        async fn collect(&self, _current_os, options, context) {
            collect_jira(&options, &context)
                .await
                .map(|entries| entries.into_activity_entries())
        }

        fn description(&self) -> &'static str {
            "Issues assigned to or reported by you"
        }

        fn instances(&self) -> crate::provider::connection::Instances {
            crate::provider::connection::Instances::Multiple
        }
    }
}
