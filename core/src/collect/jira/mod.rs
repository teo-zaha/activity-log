use anyhow::{Result, anyhow};

use crate::{
    model::{
        CollectOptions, JiraEntry, JiraEntryType,
        jira::{jira_issue::JiraIssue, jira_search_response::JiraSearchResponse},
    },
    provider::{
        jira::{JiraConfig, jira_client},
        provider_context::ProviderContext,
    },
};

pub async fn collect_jira(
    options: &CollectOptions,
    context: &ProviderContext,
) -> Result<Vec<JiraEntry>> {
    let config = context.config::<JiraConfig>()?;
    let since = options.since.format("%Y-%m-%d").to_string();
    let until = options.until.format("%Y-%m-%d").to_string();

    let client = jira_client(config)?;

    let mut entries = Vec::new();
    let mut start_at = 0;
    let max_results = 50;

    loop {
        let jql_query = jira_search_jql(&since);
        let jql = urlencoding::encode(&jql_query);
        let url = format!(
            "{}/rest/api/2/search?jql={jql}&startAt={start_at}&maxResults={max_results}&fields=summary,status,issuetype,created,updated&expand=changelog",
            config.url.trim_end_matches('/')
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

        entries.extend(map_jira_issues(&data.issues, &since, &until));
        if data.issues.len() < max_results {
            break;
        }
        start_at += max_results;
    }

    Ok(entries)
}

/// Issues touched on or after `since`. There is deliberately no upper bound on `updatedDate`:
/// an issue changed inside the range and again later is still relevant, and the changelog
/// filter in `map_jira_issues` cuts entries off at `until`.
pub(crate) fn jira_search_jql(since: &str) -> String {
    format!("updatedDate >= \"{since}\" AND (assignee = currentUser() OR reporter = currentUser())")
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

#[cfg(test)]
mod tests;
