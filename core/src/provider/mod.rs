pub mod git;
pub mod gitlab;
pub mod jira;
pub mod outlook;
pub mod provider_adapter;
pub mod provider_check_result;
pub mod provider_config;
pub mod provider_context;
pub mod provider_runtime;
pub mod system;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use crate::model::{ProvidersResponse, SupportedOs};

use self::{
    git::GitProvider, gitlab::GitLabProvider, jira::JiraProvider, outlook::OutlookProvider,
    provider_adapter::ProviderAdapter, provider_runtime::providers_response_from_runtimes,
    system::SystemProvider,
};

pub use provider_runtime::{
    ProviderRuntime, inspect_provider_runtimes, inspect_provider_runtimes_with,
};

#[macro_export]
macro_rules! provider {
    (
        $provider:ident($source:expr, $supported_os:expr, $config:ty) {
            async fn startup_check(&self, $startup_os:ident, $startup_ctx:ident) $startup_body:block
            async fn collect(&self, $collect_os:ident, $collect_options:ident, $collect_ctx:ident) $collect_body:block
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

            fn config_binding(&self) -> Option<$crate::provider::provider_config::ConfigBinding> {
                Some(<$config>::binding())
            }

            fn startup_check(
                &self,
                current_os: $crate::model::SupportedOs,
                context: $crate::provider::provider_context::ProviderContext,
            ) -> futures::future::BoxFuture<'_, $crate::provider::provider_check_result::ProviderCheckResult> {
                let $startup_os = current_os;
                let $startup_ctx = context;
                Box::pin(async move { $startup_body })
            }

            fn collect(
                &self,
                current_os: $crate::model::SupportedOs,
                options: $crate::model::CollectOptions,
                context: $crate::provider::provider_context::ProviderContext,
            ) -> futures::future::BoxFuture<'_, anyhow::Result<Vec<$crate::model::ActivityEntry>>> {
                let $collect_os = current_os;
                let $collect_options = options;
                let $collect_ctx = context;
                Box::pin(async move { $collect_body })
            }
        }
    };
    (
        $provider:ident($source:expr, $supported_os:expr) {
            async fn startup_check(&self, $startup_os:ident, $startup_ctx:ident) $startup_body:block
            async fn collect(&self, $collect_os:ident, $collect_options:ident, $collect_ctx:ident) $collect_body:block
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

            fn startup_check(
                &self,
                current_os: $crate::model::SupportedOs,
                context: $crate::provider::provider_context::ProviderContext,
            ) -> futures::future::BoxFuture<'_, $crate::provider::provider_check_result::ProviderCheckResult> {
                let $startup_os = current_os;
                let $startup_ctx = context;
                Box::pin(async move { $startup_body })
            }

            fn collect(
                &self,
                current_os: $crate::model::SupportedOs,
                options: $crate::model::CollectOptions,
                context: $crate::provider::provider_context::ProviderContext,
            ) -> futures::future::BoxFuture<'_, anyhow::Result<Vec<$crate::model::ActivityEntry>>> {
                let $collect_os = current_os;
                let $collect_options = options;
                let $collect_ctx = context;
                Box::pin(async move { $collect_body })
            }
        }
    };
}

pub fn provider_registry() -> Vec<Arc<dyn ProviderAdapter>> {
    vec![
        Arc::new(GitProvider),
        Arc::new(GitLabProvider),
        Arc::new(JiraProvider),
        Arc::new(OutlookProvider),
        Arc::new(SystemProvider),
    ]
}

pub async fn inspect_providers() -> ProvidersResponse {
    let runtimes = inspect_provider_runtimes().await;
    providers_response_from_runtimes(&runtimes)
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
