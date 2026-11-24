use std::collections::HashSet;

use anyhow::{Result, anyhow, bail};
use chrono::{DateTime, Days, FixedOffset};

use crate::{
    diagnostics,
    model::{
        CollectOptions, JiraEntry, JiraEntryType, JiraTag,
        jira::{jira_changelog::JiraChangelog, jira_issue::JiraIssue},
    },
    provider::{
        jira::{
            JiraConfig,
            api::{JiraApi, JiraResponseError},
        },
        provider_context::ProviderContext,
        remote_error::RemoteFailure,
    },
};

const PAGE_SIZE: usize = 50;

/// Jira answered a page of another offset than asked for, or only issues it had sent before:
/// read on, the search would repeat issues or never end. Its answers were readable, so this is
/// no `ApiError::Decode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PagesOutOfStep;

impl std::fmt::Display for PagesOutOfStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "Jira sent search pages out of step (results it had sent before); stopped reading",
        )
    }
}

impl std::error::Error for PagesOutOfStep {}

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
    let jql = jira_search_jql(&since);

    let api = JiraApi::new(config).map_err(|err| anyhow!(err.reason(JiraApi::SERVICE)))?;

    let mut entries = Vec::new();
    let mut partial = Vec::new();
    let mut seen = HashSet::new();
    let mut start_at = 0;

    loop {
        let mut url = api.endpoint(&["search"]);
        url.query_pairs_mut()
            .append_pair("jql", &jql)
            .append_pair("startAt", &start_at.to_string())
            .append_pair("maxResults", &PAGE_SIZE.to_string())
            .append_pair("fields", "summary,status,issuetype,created,updated")
            .append_pair("expand", "changelog");
        let page = api.search(url).await.map_err(|error| match error {
            JiraResponseError::Remote(error) => anyhow!(RemoteFailure {
                service: JiraApi::SERVICE.into(),
                error,
            }),
            JiraResponseError::Anonymous => anyhow!(error),
        })?;

        // A page of another offset, or one with nothing new, would repeat issues or never end;
        // an issue that moved into a later page is kept once.
        let received = page.issues.len();
        let fresh: Vec<JiraIssue> = page
            .issues
            .into_iter()
            .filter(|issue| seen.insert(issue.key.clone()))
            .collect();
        if page.start_at.is_some_and(|start| start != start_at)
            || (received > 0 && fresh.is_empty())
        {
            bail!(PagesOutOfStep);
        }

        partial.extend(partial_changelogs(&fresh));
        entries.extend(map_jira_issues(&fresh, options));
        match next_start(start_at, received, page.total, page.is_last) {
            Some(next) => start_at = next,
            None => break,
        }
    }

    if !partial.is_empty() {
        diagnostics::warn(
            "jira",
            format!(
                "Jira sent only part of the change history of {}; status changes outside it are missing",
                describe_issues(&partial)
            ),
        );
    }

    Ok(entries)
}

/// Where the next page starts. Paging follows what came back: the server may cap a page below
/// `PAGE_SIZE`, so only an empty page, `isLast` or reaching `total` ends it, never a short
/// page alone.
pub(crate) fn next_start(
    start_at: usize,
    received: usize,
    total: Option<usize>,
    is_last: Option<bool>,
) -> Option<usize> {
    let next = start_at + received;
    let more = received > 0 && is_last != Some(true) && total.is_none_or(|total| next < total);
    more.then_some(next)
}

/// Keys of issues whose embedded changelog is only one page of it.
pub(crate) fn partial_changelogs(issues: &[JiraIssue]) -> Vec<String> {
    issues
        .iter()
        .filter(|issue| {
            issue
                .changelog
                .as_ref()
                .is_some_and(JiraChangelog::is_partial)
        })
        .map(|issue| issue.key.clone())
        .collect()
}

/// The issues for a log line. Keys are server text, so only well-formed ones (`ABC-12`) are
/// shown, at most five; the rest are counted.
pub(crate) fn describe_issues(keys: &[String]) -> String {
    const SHOWN: usize = 5;
    let shown: Vec<&str> = keys
        .iter()
        .map(String::as_str)
        .filter(|key| is_issue_key(key))
        .take(SHOWN)
        .collect();
    let more = keys.len() - shown.len();
    match (shown.is_empty(), more) {
        (true, _) => format!("{} issues", keys.len()),
        (false, 0) => shown.join(", "),
        (false, more) => format!("{} and {more} more", shown.join(", ")),
    }
}

/// `PROJECT-123`: a project key of capitals, digits and `_` starting with a capital, a
/// number without leading zeros; at most 100 characters.
fn is_issue_key(key: &str) -> bool {
    let Some((project, number)) = key.rsplit_once('-') else {
        return false;
    };
    key.len() <= 100
        && project.starts_with(|ch: char| ch.is_ascii_uppercase())
        && project.len() >= 2
        && project
            .chars()
            .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_')
        && number.starts_with(|ch: char| matches!(ch, '1'..='9'))
        && number.chars().all(|ch| ch.is_ascii_digit())
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
        if let Some(created) = in_range(&issue.fields.created) {
            entries.push(JiraEntry {
                source: JiraTag,
                connection: crate::model::ConnectionName::unstamped(),
                id: format!("{}:created", issue.key),
                entry_type: JiraEntryType::IssueCreated,
                timestamp: created.into(),
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

                    entries.push(JiraEntry {
                        source: JiraTag,
                        connection: crate::model::ConnectionName::unstamped(),
                        id: match &history.id {
                            Some(history_id) => format!("{}:status:{history_id}", issue.key),
                            None => format!(
                                "{}:status:{}:{}",
                                issue.key,
                                changed.to_rfc3339(),
                                item.to_string.as_deref().unwrap_or_default()
                            ),
                        },
                        entry_type: JiraEntryType::StatusChanged,
                        timestamp: changed.into(),
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
            && !is_status_change(issue, updated)
        {
            entries.push(JiraEntry {
                source: JiraTag,
                connection: crate::model::ConnectionName::unstamped(),
                // Jira names only the latest change; each one seen is its own entry (cached days
                // keep earlier ones), so the instant tells them apart in any range.
                id: format!("{}:updated:{}", issue.key, updated.to_utc().to_rfc3339()),
                entry_type: JiraEntryType::IssueUpdated,
                timestamp: updated.into(),
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

/// Whether the issue's latest change, `updated`, is a status change, which is shown already:
/// the latest record of its change history is one, at `updated` (Jira stamps both alike, to
/// within its second). Decided from the changes themselves, never from the range, so every
/// partition of a span gives the same entries; an edit after a status change, however soon,
/// is an entry of its own.
fn is_status_change(issue: &JiraIssue, updated: DateTime<FixedOffset>) -> bool {
    let latest = issue
        .changelog
        .iter()
        .flat_map(|changelog| &changelog.histories)
        .filter_map(|history| Some((parse_jira_timestamp(&history.created)?, history)))
        .max_by_key(|(changed, _)| *changed);
    latest.is_some_and(|(changed, history)| {
        history
            .items
            .iter()
            .any(|item| item.field.eq_ignore_ascii_case("status"))
            && (updated - changed).abs() < chrono::Duration::seconds(1)
    })
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
