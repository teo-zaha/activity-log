pub(crate) mod api;
mod config;
pub(crate) mod scope;

#[cfg(test)]
mod tests;

use crate::{
    collect::figma::{FigmaCollectError, collect_figma},
    model::{IdentityEvidence, IntoActivityEntries, ProviderProblem, Source, SupportedOs},
    provider::{
        connection::Instances,
        provider_check_result::ProviderCheckResult,
        remote_api::{SetupError, identity_check},
    },
};
use api::FigmaApi;
#[cfg(test)]
pub(crate) use config::FIGMA_TOKEN;
pub(crate) use config::FigmaConfig;
use scope::FigmaScope;

crate::provider! {
    FigmaProvider(Source::Figma, &[SupportedOs::Windows, SupportedOs::Macos, SupportedOs::Linux], config: FigmaConfig, cache: crate::cache::CachePolicy::api()) {
        async fn startup_check(&self, _context, config) {
            check(config, FigmaApi::new).await
        }

        async fn collect(&self, options, context) {
            collect_figma(&options, &context)
                .await
                .map(|entries| entries.into_activity_entries())
        }

        fn description(&self) -> &'static str {
            "Versions you saved and comments you left in Figma files"
        }

        fn instances(&self) -> Instances {
            Instances::Multiple
        }
    }
}

async fn check(
    config: &FigmaConfig,
    client: impl FnOnce(&FigmaConfig) -> Result<FigmaApi, SetupError>,
) -> ProviderCheckResult {
    if let Err(error) = FigmaScope::of(config) {
        return ProviderCheckResult::unavailable_as(
            ProviderProblem::NotConfigured,
            error.to_string(),
        );
    }
    let api = match client(config) {
        Ok(api) => api,
        Err(error) => return error.check_result(FigmaApi::SERVICE),
    };
    match api.me().await {
        Ok(None) => ProviderCheckResult::unavailable_as(
            ProviderProblem::Failed,
            FigmaCollectError::NoIdentity.to_string(),
        )
        .with_identity(IdentityEvidence::Unnamed),
        answer => identity_check(FigmaApi::SERVICE, answer),
    }
}
