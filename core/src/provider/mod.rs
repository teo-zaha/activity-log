pub mod git;
pub mod gitlab;
pub mod jira;
pub mod outlook;
pub mod provider_adapter;
pub mod provider_check_result;
pub mod provider_collection;
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

pub const ALL_OS: &[SupportedOs] = &[SupportedOs::Windows, SupportedOs::Macos, SupportedOs::Linux];

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
