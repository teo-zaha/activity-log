use chrono::{DateTime, FixedOffset};

use crate::{
    model::{
        ActivityEntry, ActivityLog, ActivityLogMeta, CollectOptions, CollectedActivity,
        CollectedActivitySummary, IntoActivityEntries,
    },
    utils::{merge_git_with_gitlab, now_iso},
};

pub fn collect_activity_from_results(
    options: CollectOptions,
    results: Vec<(crate::model::Source, anyhow::Result<Vec<ActivityEntry>>)>,
) -> CollectedActivity {
    // Failures are logged where providers run, see `collect_activity_detailed_with_provider_runtimes`.
    let collected_entries = results
        .into_iter()
        .filter_map(|(_, result)| result.ok())
        .flatten();
    let mut git_entries = Vec::new();
    let mut gitlab_entries = Vec::new();
    let mut jira_entries = Vec::new();
    let mut outlook_entries = Vec::new();
    let mut system_entries = Vec::new();

    for entry in collected_entries {
        match entry {
            ActivityEntry::Git(entry) => git_entries.push(entry),
            ActivityEntry::GitLab(entry) => gitlab_entries.push(entry),
            ActivityEntry::Jira(entry) => jira_entries.push(entry),
            ActivityEntry::Outlook(entry) => outlook_entries.push(entry),
            ActivityEntry::System(entry) => system_entries.push(entry),
        }
    }

    // A GitLab "pushed to" event is folded into the matching git commit so the timeline does not
    // show the same change twice.
    let merged = merge_git_with_gitlab(&git_entries, &gitlab_entries);
    let merged_count = merged
        .merged_git
        .iter()
        .filter(|entry| entry.gitlab_event.is_some())
        .count();

    let summary = CollectedActivitySummary {
        git_count: merged.merged_git.len(),
        gitlab_count: merged.remaining_gitlab.len(),
        jira_count: jira_entries.len(),
        outlook_count: outlook_entries.len(),
        system_count: system_entries.len(),
        merged_count,
    };

    let mut entries = Vec::with_capacity(
        summary.git_count
            + summary.gitlab_count
            + summary.jira_count
            + summary.outlook_count
            + summary.system_count,
    );

    entries.extend(merged.merged_git.iter().cloned().into_activity_entries());
    entries.extend(
        merged
            .remaining_gitlab
            .iter()
            .cloned()
            .into_activity_entries(),
    );
    entries.extend(jira_entries.iter().cloned().into_activity_entries());
    entries.extend(outlook_entries.iter().cloned().into_activity_entries());
    entries.extend(system_entries.iter().cloned().into_activity_entries());
    entries.sort_by(|a, b| compare_timestamps_desc(a.timestamp(), b.timestamp()));

    CollectedActivity {
        summary: summary.clone(),
        log: ActivityLog {
            meta: ActivityLogMeta {
                generated: now_iso(),
                since: options.since.format("%Y-%m-%d").to_string(),
                until: options.until.format("%Y-%m-%d").to_string(),
            },
            summary,
            entries,
        },
    }
}

fn compare_timestamps_desc(a: &str, b: &str) -> std::cmp::Ordering {
    match (parse_timestamp(a), parse_timestamp(b)) {
        (Some(a_parsed), Some(b_parsed)) => b_parsed.cmp(&a_parsed).then_with(|| b.cmp(a)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => b.cmp(a),
    }
}

fn parse_timestamp(timestamp: &str) -> Option<DateTime<FixedOffset>> {
    DateTime::parse_from_rfc3339(timestamp).ok()
}

#[cfg(test)]
mod tests;
