pub(crate) mod api;
mod config;

pub use config::*;

use crate::{
    collect::gitlab::collect_gitlab,
    model::{IntoActivityEntries, Source, SupportedOs},
    provider::remote_api::identity_check,
};
use api::GitLabApi;

crate::provider! {
    GitLabProvider(Source::Gitlab, &[SupportedOs::Windows, SupportedOs::Macos, SupportedOs::Linux], config: GitLabConfig, cache: crate::cache::CachePolicy::api()) {
        async fn startup_check(&self, _current_os, _context, config) {
            let api = match GitLabApi::new(config) {
                Ok(api) => api,
                Err(err) => return err.check_result(GitLabApi::SERVICE),
            };
            identity_check(GitLabApi::SERVICE, api.current_user().await)
        }

        async fn collect(&self, _current_os, options, context) {
            collect_gitlab(&options, &context)
                .await
                .map(|entries| entries.into_activity_entries())
        }

        fn description(&self) -> &'static str {
            "Merge requests, pushes, comments and reviews"
        }

        fn instances(&self) -> crate::provider::connection::Instances {
            crate::provider::connection::Instances::Multiple
        }
    }
}
