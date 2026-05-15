mod config;

pub use config::*;

use crate::{
    collect::jira::collect_jira,
    model::{IntoActivityEntries, Source, SupportedOs},
    provider::provider_check_result::ProviderCheckResult,
};
use anyhow::{Result, anyhow};
use reqwest::{Client, header};

crate::provider! {
    JiraProvider(Source::Jira, &[SupportedOs::Windows, SupportedOs::Macos, SupportedOs::Linux], JiraConfig) {
        async fn startup_check(&self, _current_os, context) {
            let config = match context.config::<JiraConfig>() {
                Ok(c) => c,
                Err(_) => {
                    return ProviderCheckResult::unavailable("Jira config resolution failed");
                }
            };

            let client = match jira_client(config) {
                Ok(client) => client,
                Err(_) => {
                    return ProviderCheckResult::unavailable("Jira authentication setup failed");
                }
            };

            let url = format!("{}/rest/api/2/myself", config.url.trim_end_matches('/'));
            let response = client.get(url).send().await;

            match response {
                Ok(response) if response.status().is_success() => ProviderCheckResult::available(),
                Ok(response) if response.status() == reqwest::StatusCode::UNAUTHORIZED => {
                    ProviderCheckResult::unavailable("Jira credentials invalid")
                }
                Ok(response) if response.status() == reqwest::StatusCode::FORBIDDEN => {
                    ProviderCheckResult::unavailable("Jira token lacks access")
                }
                Ok(response) => {
                    ProviderCheckResult::unavailable(format!("Jira unavailable ({})", response.status()))
                }
                Err(_) => ProviderCheckResult::unavailable("Jira unreachable"),
            }
        }

        async fn collect(&self, _current_os, options, context) {
            collect_jira(&options, &context)
                .await
                .map(|entries| entries.into_activity_entries())
        }
    }
}

pub(crate) fn jira_client(config: &JiraConfig) -> Result<Client> {
    let mut headers = header::HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/json"),
    );
    headers.insert(
        header::AUTHORIZATION,
        header::HeaderValue::from_str(&format!("Bearer {}", config.token))
            .map_err(|_| anyhow!("Jira authentication setup failed"))?,
    );

    Ok(Client::builder()
        .default_headers(headers)
        .danger_accept_invalid_certs(config.insecure)
        .build()?)
}
