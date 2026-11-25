//! Current project details and merge request stacks, added after the day cache.

use std::{
    cmp::Reverse,
    collections::{BTreeMap, HashMap},
    time::Duration,
};

use futures::{StreamExt, stream};
use reqwest::Url;
use url::Origin;

use super::{ANNOTATION_CONCURRENCY, GitLabApi, GitLabConfig, lookup_failure_kind, stack};
use crate::{
    collect::{
        change_stack::{Budget, ChangeRef, FailureKind, LookupCause, LookupFailure},
        web_link::{Unshown, web_link},
    },
    diagnostics,
    model::{ActivityEntry, ChangeStack, GitLabEntry},
    provider::{provider_context::ProviderContext, remote_api::RequestBudget},
};

const PROJECT_REQUESTS: usize = 20;
const PROJECT_TIME: Duration = Duration::from_secs(5);

pub(super) async fn annotate(entries: &mut [ActivityEntry], context: ProviderContext) {
    let (projects, stacks) = if entries
        .iter()
        .any(|entry| matches!(entry, ActivityEntry::GitLab(entry) if entry.project_id.is_some()))
        && let Some(api) = api(&context)
    {
        let (report, stacks) = look_up(entries, &api).await;
        report.log();
        (report.projects, stacks)
    } else {
        Default::default()
    };
    // Publish together after every await, and clear details left from a previous annotation.
    for entry in entries {
        if let ActivityEntry::GitLab(entry) = entry {
            let project = entry.project_id.and_then(|id| projects.get(&id));
            entry.project = project.map(|project| project.name.clone());
            entry.web_url = project.and_then(|project| project.item_url(entry));
            entry.change_stack =
                stack::change_of(entry).and_then(|change| stacks.get(&change).cloned());
        }
    }
}

async fn look_up(
    entries: &[ActivityEntry],
    api: &GitLabApi,
) -> (ProjectReport, HashMap<ChangeRef<u64>, ChangeStack>) {
    let budget = Budget::for_collection();
    let mut latest = BTreeMap::new();
    let mut changes = Vec::new();
    for entry in entries {
        let ActivityEntry::GitLab(entry) = entry else {
            continue;
        };
        let Some(id) = entry.project_id else {
            continue;
        };
        let at = entry.timestamp.utc();
        let last = latest.entry(id).or_insert(at);
        *last = (*last).max(at);
        if let Some(change) = stack::change_of(entry) {
            changes.push((change, at));
        }
    }
    let mut order: Vec<_> = latest.into_iter().collect();
    order.sort_unstable_by_key(|&(id, at)| (Reverse(at), id));
    let beyond_allowance = order.len().saturating_sub(PROJECT_REQUESTS);
    let ids = order
        .into_iter()
        .take(PROJECT_REQUESTS)
        .map(|(id, _)| id)
        .collect();

    let mut projects = read_projects(ids, api, &budget.phase(PROJECT_REQUESTS, PROJECT_TIME)).await;
    if beyond_allowance > 0 {
        projects.failures.entry(FailureKind::Limit).or_default().0 += beyond_allowance;
    }
    let stacks = stack::look_up(changes, api, &budget).await;
    (projects, stacks)
}

fn api(context: &ProviderContext) -> Option<GitLabApi> {
    let config = match context.config::<GitLabConfig>() {
        Ok(config) => config,
        Err(error) => {
            diagnostics::warn("gitlab", format!("Skipped GitLab details ({error:#})"));
            return None;
        }
    };
    match GitLabApi::new(config) {
        Ok(api) => Some(api),
        Err(error) => {
            diagnostics::warn(
                "gitlab",
                format!(
                    "Skipped GitLab details ({})",
                    error.reason(GitLabApi::SERVICE)
                ),
            );
            None
        }
    }
}

struct ProjectDetails {
    name: String,
    web_base: Option<Url>,
}

impl ProjectDetails {
    fn item_url(&self, entry: &GitLabEntry) -> Option<String> {
        if entry.entry_type.is_push() {
            return None;
        }
        let (route, iid) = match (entry.mr_iid, entry.issue_iid) {
            (Some(iid), None) => ("merge_requests", iid),
            (None, Some(iid)) => ("issues", iid),
            _ => return None,
        };
        let mut url = self.web_base.clone()?;
        url.path_segments_mut()
            .ok()?
            .pop_if_empty()
            .extend(["-", route, &iid.to_string()]);
        Some(url.into())
    }
}

#[derive(Default)]
struct ProjectReport {
    projects: HashMap<u64, ProjectDetails>,
    failures: BTreeMap<FailureKind, (usize, Option<LookupCause>)>,
    elsewhere: Option<(usize, Origin)>,
    requests: usize,
}

impl ProjectReport {
    fn log(&self) {
        let noun = if self.requests == 1 {
            "request"
        } else {
            "requests"
        };
        diagnostics::info(
            "gitlab",
            format!("GitLab project lookups used {} {noun}.", self.requests),
        );
        if !self.failures.is_empty() {
            let reasons: Vec<_> = self
                .failures
                .iter()
                .map(|(kind, (count, cause))| {
                    let reason = match kind {
                        FailureKind::Unreadable => "unreadable",
                        FailureKind::TokenInvalid => "token refused",
                        FailureKind::ScopeMissing => "scope missing",
                        FailureKind::Throttled => "throttled",
                        FailureKind::Limit => "time or request limit reached",
                        FailureKind::Failed => "request failed",
                    };
                    let cause = cause
                        .as_ref()
                        .map(|cause| format!(" ({})", cause.message(GitLabApi::SERVICE)))
                        .unwrap_or_default();
                    format!("{count} {reason}{cause}")
                })
                .collect();
            diagnostics::warn(
                "gitlab",
                format!(
                    "GitLab project details were unavailable ({})",
                    reasons.join(", ")
                ),
            );
        }
        if let Some((count, origin)) = &self.elsewhere {
            diagnostics::warn(
                "gitlab",
                format!(
                    "Skipped GitLab project URLs on another origin ({count} affected). The first was {}.",
                    origin.ascii_serialization()
                ),
            );
        }
    }
}

async fn read_projects(ids: Vec<u64>, api: &GitLabApi, budget: &Budget) -> ProjectReport {
    let origin = api.web_origin();
    let mut results = stream::iter(ids.into_iter().map(|id| async move {
        let result = budget
            .run(async {
                budget.try_spend()?;
                api.project(id)
                    .await
                    .map_err(|error| LookupFailure::caused_by(lookup_failure_kind(&error), error))
            })
            .await;
        (id, result)
    }))
    .buffered(ANNOTATION_CONCURRENCY);
    let mut report = ProjectReport::default();
    while let Some((id, response)) = results.next().await {
        match response {
            Ok(project) => {
                let web_base = project
                    .web_url
                    .and_then(|raw| match web_link(&raw, &origin) {
                        Ok(link) => Some(link),
                        Err(Unshown::Elsewhere(other)) => {
                            let (count, _) = report.elsewhere.get_or_insert((0, other));
                            *count += 1;
                            None
                        }
                        Err(Unshown::NotAWebPage) => None,
                    })
                    .and_then(|link| Url::parse(link.as_str()).ok())
                    .map(|mut url| {
                        url.set_query(None);
                        url.set_fragment(None);
                        url
                    });
                report.projects.insert(
                    id,
                    ProjectDetails {
                        name: project.path_with_namespace,
                        web_base,
                    },
                );
            }
            Err(failure) => {
                let (count, cause) = report.failures.entry(failure.kind()).or_default();
                *count += 1;
                if cause.is_none() {
                    *cause = failure.cause().cloned();
                }
            }
        }
    }
    ProjectReport {
        requests: budget.spent(),
        ..report
    }
}

#[cfg(test)]
mod tests;
