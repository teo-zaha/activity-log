pub(crate) mod api;
mod config;
mod stack;

pub use config::*;

use crate::{
    collect::gitlab::collect_gitlab,
    model::{IntoActivityEntries, Source, SupportedOs},
    provider::remote_api::identity_check,
};
use api::GitLabApi;

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
            Box::pin(stack::annotate(entries, context))
        }
    }
}
