use futures::future::BoxFuture;

use crate::{
    collect::gitlab::{collect_gitlab, gitlab_client_context},
    model::{CollectOptions, Source, SupportedOs},
    provider::{
        ALL_OS, provider_adapter::ProviderAdapter, provider_check_result::ProviderCheckResult,
        provider_collection::ProviderCollection, provider_config::ProviderConfigKey,
        provider_context::ProviderContext,
    },
};

pub struct GitLabProvider;

pub const GITLAB_URL: ProviderConfigKey = ProviderConfigKey::required_url("GITLAB_URL");
pub const GITLAB_TOKEN: ProviderConfigKey = ProviderConfigKey::required_password("GITLAB_TOKEN");
pub const GITLAB_USER: ProviderConfigKey = ProviderConfigKey::required_string("GITLAB_USER");

const CONFIG_KEYS: &[ProviderConfigKey] = &[GITLAB_URL, GITLAB_TOKEN, GITLAB_USER];

impl ProviderAdapter for GitLabProvider {
    fn id(&self) -> Source {
        Source::Gitlab
    }

    fn supported_os(&self) -> &'static [SupportedOs] {
        ALL_OS
    }

    fn config_requirements(&self) -> &'static [ProviderConfigKey] {
        CONFIG_KEYS
    }

    fn startup_check(&self, context: ProviderContext) -> BoxFuture<'_, ProviderCheckResult> {
        Box::pin(async move {
            let (client, gitlab_url, gitlab_token) = match gitlab_client_context(&context) {
                Ok(values) => values,
                Err(_) => {
                    return ProviderCheckResult::unavailable("GitLab client setup failed");
                }
            };
            let url = format!("{}/api/v4/user", gitlab_url.trim_end_matches('/'));
            let response = client
                .get(url)
                .header("PRIVATE-TOKEN", gitlab_token)
                .send()
                .await;

            match response {
                Ok(response) if response.status().is_success() => ProviderCheckResult::available(),
                Ok(response) if response.status() == reqwest::StatusCode::UNAUTHORIZED => {
                    ProviderCheckResult::unavailable("GitLab credentials invalid")
                }
                Ok(response) => ProviderCheckResult::unavailable(format!(
                    "GitLab unavailable ({})",
                    response.status()
                )),
                Err(_) => ProviderCheckResult::unavailable("GitLab unreachable"),
            }
        })
    }

    fn collect(
        &self,
        options: CollectOptions,
        context: ProviderContext,
    ) -> BoxFuture<'_, anyhow::Result<ProviderCollection>> {
        Box::pin(async move {
            collect_gitlab(&options, &context)
                .await
                .map(ProviderCollection::GitLab)
        })
    }
}
