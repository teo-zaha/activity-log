pub(crate) mod api;
mod config;

pub use config::*;

use crate::{
    collect::github::collect_github,
    model::{IntoActivityEntries, Source, SupportedOs},
    provider::remote_api::identity_check,
};
use api::GitHubApi;

crate::provider! {
    GitHubProvider(
        Source::Github,
        &[SupportedOs::Windows, SupportedOs::Macos, SupportedOs::Linux],
        config: GitHubConfig,
        cache: crate::cache::CachePolicy::api_with_settled_ttl(
            std::time::Duration::from_secs(30 * 24 * 60 * 60)
        )
    ) {
        async fn startup_check(&self, _context, config) {
            let api = match GitHubApi::new(config) {
                Ok(api) => api,
                Err(error) => return error.check_result(GitHubApi::SERVICE),
            };
            identity_check(GitHubApi::SERVICE, api.current_user().await)
        }

        async fn collect(&self, options, context) {
            collect_github(&options, &context)
                .await
                .map(|entries| entries.into_activity_entries())
        }

        fn description(&self) -> &'static str {
            "Pull requests, reviews and issues"
        }

        fn instances(&self) -> crate::provider::connection::Instances {
            crate::provider::connection::Instances::Multiple
        }
    }
}
