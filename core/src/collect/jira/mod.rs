use anyhow::{Result, anyhow};
use chrono::{DateTime, Days, FixedOffset};

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
    // JQL dates are read in the Jira user's timezone, which can be a day behind UTC; the exact
    // range is applied to every entry in `map_jira_issues`.
    let since = (options.start.date_naive() - Days::new(1))
        .format("%Y-%m-%d")
        .to_string();

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

        entries.extend(map_jira_issues(&data.issues, options));
        if data.issues.len() < max_results {
            break;
        }
        start_at += max_results;
    }

    Ok(entries)
}

/// Issues touched on or after `since`. There is deliberately no upper bound on `updatedDate`:
/// an issue changed inside the range and again later is still relevant, and the changelog
/// filter in `map_jira_issues` cuts entries off at the end of the range.
pub(crate) fn jira_search_jql(since: &str) -> String {
    format!("updatedDate >= \"{since}\" AND (assignee = currentUser() OR reporter = currentUser())")
}

fn map_jira_issues(issues: &[JiraIssue], options: &CollectOptions) -> Vec<JiraEntry> {
    let in_range = |raw: &str| parse_jira_timestamp(raw).filter(|at| options.contains(at.to_utc()));
    let mut entries = Vec::new();

    for issue in issues {
        let mut emitted_status_change = false;

        if let Some(created) = in_range(&issue.fields.created) {
            entries.push(JiraEntry {
                source: "jira".to_string(),
                entry_type: JiraEntryType::IssueCreated,
                timestamp: created.to_rfc3339(),
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
                let Some(changed) = in_range(&history.created) else {
                    continue;
                };

                for item in &history.items {
                    if !item.field.eq_ignore_ascii_case("status") {
                        continue;
                    }

                    emitted_status_change = true;
                    entries.push(JiraEntry {
                        source: "jira".to_string(),
                        entry_type: JiraEntryType::StatusChanged,
                        timestamp: changed.to_rfc3339(),
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

        if let Some(updated) = in_range(&issue.fields.updated)
            && issue.fields.updated != issue.fields.created
            && !emitted_status_change
        {
            entries.push(JiraEntry {
                source: "jira".to_string(),
                entry_type: JiraEntryType::IssueUpdated,
                timestamp: updated.to_rfc3339(),
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

/// Jira writes offsets without a colon (`2026-03-05T08:00:00.000+0100`), which is not RFC 3339;
/// entries are re-emitted as RFC 3339 so every client can parse them.
pub(crate) fn parse_jira_timestamp(raw: &str) -> Option<DateTime<FixedOffset>> {
    DateTime::parse_from_rfc3339(raw)
        .or_else(|_| DateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S%.f%z"))
        .ok()
}

#[cfg(test)]
mod tests;
