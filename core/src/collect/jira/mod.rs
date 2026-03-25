use anyhow::{Result, anyhow};
use reqwest::{Client, header};

use crate::{
    cache::{get_cached, set_cached},
    collect::load_required_envs,
    model::{
        CollectOptions, JiraEntry, JiraEntryType,
        jira::{
            jira_issue::JiraIssue, jira_search_response::JiraSearchResponse,
        },
    },
};

pub async fn collect_jira(options: &CollectOptions) -> Result<Vec<JiraEntry>> {
    let Some(env_values) = load_required_envs("jira", &["JIRA_URL", "JIRA_EMAIL", "JIRA_TOKEN"]) else {
        return Ok(Vec::new());
    };
    let jira_url = &env_values[0];
    let jira_email = &env_values[1];
    let jira_token = &env_values[2];

    let cache_key = format!("jira-{}-{}", options.since, options.until);
    if let Some(cached) = get_cached::<Vec<JiraEntry>>(&cache_key)? {
        return Ok(cached);
    }

    let insecure = std::env::var("JIRA_INSECURE")
        .map(|value| value.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let auth_type = std::env::var("JIRA_AUTH_TYPE")
        .unwrap_or_else(|_| "bearer".to_string())
        .to_lowercase();

    let mut headers = header::HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/json"),
    );

    let auth_header = if auth_type == "basic" {
        let credentials = format!("{jira_email}:{jira_token}");
        format!("Basic {}", base64_encode(credentials.as_bytes()))
    } else {
        format!("Bearer {jira_token}")
    };
    headers.insert(
        header::AUTHORIZATION,
        header::HeaderValue::from_str(&auth_header)?,
    );

    let client = Client::builder()
        .default_headers(headers)
        .danger_accept_invalid_certs(insecure)
        .build()?;

    let mut entries = Vec::new();
    let mut start_at = 0;
    let max_results = 50;

    loop {
        let jql_query = format!(
            "updatedDate >= \"{}\" AND updatedDate <= \"{}\" AND (assignee = currentUser() OR reporter = currentUser())",
            options.since, options.until
        );
        let jql = urlencoding::encode(&jql_query);
        let url = format!(
            "{}/rest/api/2/search?jql={jql}&startAt={start_at}&maxResults={max_results}&fields=summary,status,issuetype,created,updated&expand=changelog",
            jira_url.trim_end_matches('/')
        );
        let response = client.get(url).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!("[jira] API error {status}: {body}"));
        }

        let data = response.json::<JiraSearchResponse>().await?;
        if data.issues.is_empty() {
            break;
        }

        entries.extend(map_jira_issues(&data.issues, &options.since, &options.until));
        if data.issues.len() < max_results {
            break;
        }
        start_at += max_results;
    }

    set_cached(&cache_key, &entries)?;
    Ok(entries)
}

fn map_jira_issues(issues: &[JiraIssue], since: &str, until: &str) -> Vec<JiraEntry> {
    let mut entries = Vec::new();

    for issue in issues {
        let mut emitted_status_change = false;

        if is_timestamp_in_range(&issue.fields.created, since, until) {
            entries.push(JiraEntry {
                source: "jira".to_string(),
                entry_type: JiraEntryType::IssueCreated,
                timestamp: issue.fields.created.clone(),
                key: issue.key.clone(),
                title: issue.fields.summary.clone(),
                status: issue.fields.status.name.clone(),
                issue_type: issue.fields.issuetype.name.clone(),
                from_status: None,
                to_status: None,
            });
        }

        if let Some(changelog) = &issue.changelog {
            for history in &changelog.histories {
                if !is_timestamp_in_range(&history.created, since, until) {
                    continue;
                }

                for item in &history.items {
                    if !item.field.eq_ignore_ascii_case("status") {
                        continue;
                    }

                    emitted_status_change = true;
                    entries.push(JiraEntry {
                        source: "jira".to_string(),
                        entry_type: JiraEntryType::StatusChanged,
                        timestamp: history.created.clone(),
                        key: issue.key.clone(),
                        title: issue.fields.summary.clone(),
                        status: item
                            .to_string
                            .clone()
                            .unwrap_or_else(|| issue.fields.status.name.clone()),
                        issue_type: issue.fields.issuetype.name.clone(),
                        from_status: item.from_string.clone(),
                        to_status: item.to_string.clone(),
                    });
                }
            }
        }

        if is_timestamp_in_range(&issue.fields.updated, since, until)
            && issue.fields.updated != issue.fields.created
            && !emitted_status_change
        {
            entries.push(JiraEntry {
                source: "jira".to_string(),
                entry_type: JiraEntryType::IssueUpdated,
                timestamp: issue.fields.updated.clone(),
                key: issue.key.clone(),
                title: issue.fields.summary.clone(),
                status: issue.fields.status.name.clone(),
                issue_type: issue.fields.issuetype.name.clone(),
                from_status: None,
                to_status: None,
            });
        }
    }

    entries
}

fn is_timestamp_in_range(timestamp: &str, since: &str, until: &str) -> bool {
    timestamp
        .get(0..10)
        .map(|date| date >= since && date <= until)
        .unwrap_or(false)
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

#[cfg(test)]
mod tests;
