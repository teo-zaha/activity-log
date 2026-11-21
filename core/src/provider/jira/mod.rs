pub(crate) mod api;
mod config;

pub use config::*;

use crate::{
    collect::jira::collect_jira,
    model::{IntoActivityEntries, Source, SupportedOs},
    provider::remote_api::identity_check,
};
use api::JiraApi;

crate::provider! {
    JiraProvider(Source::Jira, &[SupportedOs::Windows, SupportedOs::Macos, SupportedOs::Linux], config: JiraConfig, cache: crate::cache::CachePolicy::api()) {
        async fn startup_check(&self, _context, config) {
            let api = match JiraApi::new(config) {
                Ok(api) => api,
                Err(err) => return err.check_result(JiraApi::SERVICE),
            };
            identity_check(JiraApi::SERVICE, api.myself().await)
        }

        async fn collect(&self, options, context) {
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
