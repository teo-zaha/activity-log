pub(crate) mod api;
mod config;
pub(crate) mod scope;

#[cfg(test)]
mod tests;

use std::{future::Future, sync::Arc};

use crate::{
    collect::figma::{FigmaCollection, collect_figma, file_book, local_tabs},
    model::{IdentityEvidence, IntoActivityEntries, ProviderProblem, Source, SupportedOs},
    provider::{
        connection::Instances,
        provider_adapter::CollectionState,
        provider_check_result::ProviderCheckResult,
        provider_context::ProviderContext,
        remote_api::{SetupError, identity_check, missing_identity},
    },
};
use api::FigmaApi;
#[cfg(test)]
pub(crate) use config::FIGMA_TOKEN;
pub(crate) use config::FigmaConfig;
use scope::FigmaScope;

crate::provider! {
    FigmaProvider(Source::Figma, &[SupportedOs::Windows, SupportedOs::Macos, SupportedOs::Linux], config: FigmaConfig, cache: crate::cache::CachePolicy::api()) {
        async fn startup_check(&self, context, config) {
            check(&context, config, FigmaApi::new, local_tabs::find()).await
        }

        async fn collect(&self, options, context) {
            collect_figma(&options, &context)
                .await
                .map(|entries| entries.into_activity_entries())
        }

        fn credential_setup(&self) -> Option<&'static crate::provider::CredentialSetup> {
            Some(&config::CREDENTIAL_SETUP)
        }

        fn description(&self) -> &'static str {
            "Versions you saved and comments you left in Figma files"
        }

        fn instances(&self) -> Instances {
            Instances::Multiple
        }

        fn collection_state(&self) -> Option<CollectionState> {
            Some(Arc::new(FigmaCollection::default()))
        }
    }
}

async fn check(
    context: &ProviderContext,
    config: &FigmaConfig,
    client: impl FnOnce(&FigmaConfig) -> Result<FigmaApi, SetupError>,
    tabs: impl Future<Output = local_tabs::Outcome>,
) -> ProviderCheckResult {
    let scope = match FigmaScope::of(config) {
        Ok(scope) => scope,
        Err(error) => {
            return ProviderCheckResult::unavailable_as(
                ProviderProblem::NotConfigured,
                error.to_string(),
            );
        }
    };
    let api = match client(config) {
        Ok(api) => api,
        Err(error) => return error.check_result(FigmaApi::SERVICE),
    };
    match api.me().await {
        Ok(Some(principal)) => {
            if scope.files.is_empty() && scope.teams.is_empty() {
                let tabs = tabs.await;
                if tabs.files_for(&principal.id).next().is_none() {
                    let remembered = match file_book::path(context, &principal) {
                        Some(path) => file_book::has_recent_tab_file(path).await,
                        None => Ok(false),
                    };
                    let unavailable = match remembered {
                        Ok(true) => return identity_check(FigmaApi::SERVICE, Ok(Some(principal))),
                        Ok(false) => missing_files(&tabs),
                        Err(error) => ProviderCheckResult::unavailable_from(&error.into()),
                    };
                    return unavailable.with_identity(IdentityEvidence::Verified(principal));
                }
            }
            identity_check(FigmaApi::SERVICE, Ok(Some(principal)))
        }
        Ok(None) => missing_identity(
            FigmaApi::SERVICE,
            "Activity Log can't identify your versions or comments. Use Check again to retry.",
        ),
        Err(error) => identity_check(FigmaApi::SERVICE, Err(error)),
    }
}

fn missing_files(tabs: &local_tabs::Outcome) -> ProviderCheckResult {
    let reason = if local_tabs::SUPPORTED {
        let explanation = tabs.warning().unwrap_or_else(|| {
            "No file links, teams, matching open tabs or remembered files are available for this Figma account.".into()
        });
        format!("{explanation} Add a file link or team, or open a file in the Figma desktop app.")
    } else {
        "No Figma file links, teams or remembered files are available for this account. Add a file link or team. Desktop tab discovery is available on macOS and Windows.".into()
    };
    ProviderCheckResult::unavailable_as(ProviderProblem::NotConfigured, reason)
}
