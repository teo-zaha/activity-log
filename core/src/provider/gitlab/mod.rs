mod config;

pub use config::*;

use crate::{
    collect::gitlab::collect_gitlab,
    model::{IntoActivityEntries, Source, SupportedOs},
    provider::provider_check_result::ProviderCheckResult,
};

crate::provider! {
    GitLabProvider(Source::Gitlab, &[SupportedOs::Windows, SupportedOs::Macos, SupportedOs::Linux], GitLabConfig) {
        async fn startup_check(&self, _current_os, context) {
            let config = match context.config::<GitLabConfig>() {
                Ok(c) => c,
                Err(_) => {
                    return ProviderCheckResult::unavailable("GitLab config resolution failed");
                }
            };

            let client = match reqwest::Client::builder().build() {
                Ok(client) => client,
                Err(_) => {
                    return ProviderCheckResult::unavailable("GitLab client setup failed");
                }
            };
            let url = format!("{}/api/v4/user", config.url.trim_end_matches('/'));
            let response = client
                .get(url)
                .header("PRIVATE-TOKEN", config.token.as_str())
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
        }

        async fn collect(&self, _current_os, options, context) {
            collect_gitlab(&options, &context)
                .await
                .map(|entries| entries.into_activity_entries())
        }
    }
}
