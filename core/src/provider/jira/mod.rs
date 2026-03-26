use futures::future::BoxFuture;

use crate::{
    collect::jira::{collect_jira, jira_client_context},
    model::{CollectOptions, Source, SupportedOs},
    provider::{
        ALL_OS, provider_adapter::ProviderAdapter, provider_check_result::ProviderCheckResult,
        provider_collection::ProviderCollection, provider_config::ProviderConfigKey,
        provider_context::ProviderContext,
    },
};

pub struct JiraProvider;

pub const JIRA_URL: ProviderConfigKey = ProviderConfigKey::required_url("JIRA_URL");
pub const JIRA_EMAIL: ProviderConfigKey = ProviderConfigKey::required_email("JIRA_EMAIL");
pub const JIRA_TOKEN: ProviderConfigKey = ProviderConfigKey::required_password("JIRA_TOKEN");
pub const JIRA_AUTH_TYPE: ProviderConfigKey = ProviderConfigKey::optional_string("JIRA_AUTH_TYPE");
pub const JIRA_INSECURE: ProviderConfigKey = ProviderConfigKey::optional_bool("JIRA_INSECURE");

const CONFIG_KEYS: &[ProviderConfigKey] = &[
    JIRA_URL,
    JIRA_EMAIL,
    JIRA_TOKEN,
    JIRA_AUTH_TYPE,
    JIRA_INSECURE,
];

impl ProviderAdapter for JiraProvider {
    fn id(&self) -> Source {
        Source::Jira
    }

    fn supported_os(&self) -> &'static [SupportedOs] {
        ALL_OS
    }

    fn config_requirements(&self) -> &'static [ProviderConfigKey] {
        CONFIG_KEYS
    }

    fn startup_check(&self, context: ProviderContext) -> BoxFuture<'_, ProviderCheckResult> {
        Box::pin(async move {
            let (client, jira_url) = match jira_client_context(&context) {
                Ok(values) => values,
                Err(err) => {
                    let reason = err.to_string();
                    if reason.contains("authentication setup failed") {
                        return ProviderCheckResult::unavailable(
                            "Jira authentication setup failed",
                        );
                    }
                    return ProviderCheckResult::unavailable("Jira client setup failed");
                }
            };

            let url = format!("{}/rest/api/2/myself", jira_url.trim_end_matches('/'));
            let response = client.get(url).send().await;

            match response {
                Ok(response) if response.status().is_success() => ProviderCheckResult::available(),
                Ok(response) if response.status() == reqwest::StatusCode::UNAUTHORIZED => {
                    ProviderCheckResult::unavailable("Jira credentials invalid")
                }
                Ok(response) => ProviderCheckResult::unavailable(format!(
                    "Jira unavailable ({})",
                    response.status()
                )),
                Err(_) => ProviderCheckResult::unavailable("Jira unreachable"),
            }
        })
    }

    fn collect(
        &self,
        options: CollectOptions,
        context: ProviderContext,
    ) -> BoxFuture<'_, anyhow::Result<ProviderCollection>> {
        Box::pin(async move {
            collect_jira(&options, &context)
                .await
                .map(ProviderCollection::Jira)
        })
    }
}
