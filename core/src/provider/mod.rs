pub mod calendar;
pub mod connection;
pub mod git;
pub mod github;
pub mod gitlab;
pub mod jira;
pub mod outlook;
pub mod provider_adapter;
pub mod provider_check_result;
pub mod provider_config;
pub mod provider_context;
pub mod provider_runtime;
pub(crate) mod remote_api;
pub mod remote_error;
pub mod source_failure;
pub mod system;

#[cfg(test)]
mod figma;

#[cfg(test)]
mod tests;

use std::{
    sync::{Arc, LazyLock},
    time::Duration,
};

use strum::IntoEnumIterator;

use crate::model::{Source, SupportedOs};

use self::{
    calendar::CalendarProvider, git::GitProvider, github::GitHubProvider, gitlab::GitLabProvider,
    jira::JiraProvider, outlook::OutlookProvider, provider_adapter::ProviderAdapter,
    system::SystemProvider,
};

pub use provider_runtime::{
    ProviderRuntime, inspect_provider_runtimes_with, start_check, start_checks,
};

/// HTTP client defaults for remote providers; reqwest has no timeouts by default.
pub(crate) fn http_client_builder() -> reqwest::ClientBuilder {
    let builder = reqwest::Client::builder()
        .user_agent(concat!("activity-log/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30));
    // Tests talk to local servers only; a proxy from the environment must never see them.
    #[cfg(test)]
    let builder = builder.no_proxy();
    builder
}

/// Declares a provider kind's adapter: its source, the OSes it runs on, optionally its typed
/// settings (`config: GitLabConfig`, which its startup check gets as its third argument) and its
/// day cache (`cache: CachePolicy::api()`), its startup check, its collection, and any other
/// `ProviderAdapter` method.
#[macro_export]
macro_rules! provider {
    (
        $provider:ident($source:expr, $supported_os:expr $(, config: $config:ty)? $(, cache: $cache:expr)?) {
            async fn startup_check(&self, $startup_ctx:ident $(, $startup_config:ident)?) $startup_body:block
            async fn collect(&self, $collect_options:ident, $collect_ctx:ident) $collect_body:block
            $($rest:tt)*
        }
    ) => {
        pub struct $provider;

        impl $crate::provider::provider_adapter::ProviderAdapter for $provider {
            fn id(&self) -> $crate::model::Source {
                $source
            }

            fn supported_os(&self) -> &'static [$crate::model::SupportedOs] {
                $supported_os
            }

            $(
                fn config_binding(&self) -> Option<$crate::provider::provider_config::ConfigBinding> {
                    Some(<$config>::binding())
                }
            )?

            $(
                fn cache_policy(&self) -> Option<$crate::cache::CachePolicy> {
                    Some($cache)
                }
            )?

            fn startup_check(
                &self,
                context: $crate::provider::provider_context::ProviderContext,
            ) -> futures::future::BoxFuture<
                '_,
                $crate::provider::provider_check_result::ProviderCheckResult,
            > {
                let $startup_ctx = context;
                Box::pin(async move {
                    // The runtime resolves the settings before it checks, and checks only
                    // what resolved; a context it did not resolve says why.
                    $(
                        let $startup_config = match $startup_ctx.config::<$config>() {
                            Ok(config) => config,
                            Err(err) => {
                                return $crate::provider::provider_check_result::ProviderCheckResult::unavailable_as(
                                    $crate::model::ProviderProblem::NotConfigured,
                                    format!("{err:#}"),
                                );
                            }
                        };
                    )?
                    $startup_body
                })
            }

            fn collect(
                &self,
                options: $crate::model::CollectOptions,
                context: $crate::provider::provider_context::ProviderContext,
            ) -> futures::future::BoxFuture<'_, anyhow::Result<Vec<$crate::model::ActivityEntry>>>
            {
                let $collect_options = options;
                let $collect_ctx = context;
                Box::pin(async move { $collect_body })
            }

            $($rest)*
        }
    };
}

/// Every kind's adapter, in `Source` order.
static REGISTRY: LazyLock<Vec<Arc<dyn ProviderAdapter>>> = LazyLock::new(|| {
    Source::iter()
        .map(|source| -> Arc<dyn ProviderAdapter> {
            match source {
                Source::Git => Arc::new(GitProvider),
                Source::Gitlab => Arc::new(GitLabProvider),
                Source::Github => Arc::new(GitHubProvider),
                Source::Jira => Arc::new(JiraProvider),
                Source::Outlook => Arc::new(OutlookProvider),
                Source::System => Arc::new(SystemProvider),
                Source::Calendar => Arc::new(CalendarProvider),
            }
        })
        .collect()
});

/// Every kind's adapter, in `Source` order.
pub fn provider_registry() -> &'static [Arc<dyn ProviderAdapter>] {
    &REGISTRY
}

/// The adapter of `source`; every kind has one.
pub fn adapter(source: &Source) -> &'static Arc<dyn ProviderAdapter> {
    let index = Source::iter()
        .position(|kind| kind == *source)
        .expect("`Source::iter` yields every kind");
    &REGISTRY[index]
}

pub fn current_os() -> SupportedOs {
    if cfg!(target_os = "windows") {
        SupportedOs::Windows
    } else if cfg!(target_os = "macos") {
        SupportedOs::Macos
    } else {
        SupportedOs::Linux
    }
}
