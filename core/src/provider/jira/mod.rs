mod config;

pub use config::*;

use crate::{
    collect::jira::collect_jira,
    model::{IntoActivityEntries, Source, SupportedOs},
    provider::provider_check_result::ProviderCheckResult,
};

crate::provider! {
    JiraProvider(Source::Jira, &[SupportedOs::Windows, SupportedOs::Macos, SupportedOs::Linux], JiraConfig) {
        async fn startup_check(&self, _current_os, context) {
            let config = match context.config::<JiraConfig>() {
                Ok(c) => c,
                Err(_) => {
                    return ProviderCheckResult::unavailable("Jira config resolution failed");
                }
            };

            let mut headers = reqwest::header::HeaderMap::new();
            headers.insert(
                reqwest::header::CONTENT_TYPE,
                reqwest::header::HeaderValue::from_static("application/json"),
            );

            let auth_header = if config.auth_type.eq_ignore_ascii_case("basic") {
                let credentials = format!("{}:{}", config.email, config.token);
                format!("Basic {}", base64_encode(credentials.as_bytes()))
            } else {
                format!("Bearer {}", config.token)
            };
            headers.insert(
                reqwest::header::AUTHORIZATION,
                match reqwest::header::HeaderValue::from_str(&auth_header) {
                    Ok(value) => value,
                    Err(_) => {
                        return ProviderCheckResult::unavailable(
                            "Jira authentication setup failed",
                        );
                    }
                },
            );

            let client = match reqwest::Client::builder()
                .default_headers(headers)
                .danger_accept_invalid_certs(config.insecure)
                .build()
            {
                Ok(client) => client,
                Err(err) => {
                    let reason = err.to_string();
                    if reason.contains("authentication setup failed") {
                        return ProviderCheckResult::unavailable("Jira authentication setup failed");
                    }
                    return ProviderCheckResult::unavailable("Jira client setup failed");
                }
            };

            let url = format!("{}/rest/api/2/myself", config.url.trim_end_matches('/'));
            let response = client.get(url).send().await;

            match response {
                Ok(response) if response.status().is_success() => ProviderCheckResult::available(),
                Ok(response) if response.status() == reqwest::StatusCode::UNAUTHORIZED => {
                    ProviderCheckResult::unavailable("Jira credentials invalid")
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

fn base64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::new();
    let mut index = 0;
    while index < input.len() {
        let a = input[index];
        let b = *input.get(index + 1).unwrap_or(&0);
        let c = *input.get(index + 2).unwrap_or(&0);

        output.push(TABLE[(a >> 2) as usize] as char);
        output.push(TABLE[(((a & 0b11) << 4) | (b >> 4)) as usize] as char);
        output.push(if index + 1 < input.len() {
            TABLE[(((b & 0b1111) << 2) | (c >> 6)) as usize] as char
        } else {
            '='
        });
        output.push(if index + 2 < input.len() {
            TABLE[(c & 0b11_1111) as usize] as char
        } else {
            '='
        });
        index += 3;
    }
    output
}
