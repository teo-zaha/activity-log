use crate::{
    model::{
        ActivityEntry, ActivityLog, ActivityLogMeta, CollectOptions, CollectedActivity,
        CollectedActivitySummary,
        IntoActivityEntries,
    },
    utils::{merge_git_with_gitlab, now_iso},
};

pub fn collect_activity_from_results(
    options: CollectOptions,
    results: Vec<(crate::model::Source, anyhow::Result<Vec<ActivityEntry>>)>,
) -> CollectedActivity {
    let mut collected_entries = Vec::new();
    let mut git_entries = Vec::new();
    let mut gitlab_entries = Vec::new();
    let mut jira_entries = Vec::new();
    let mut outlook_entries = Vec::new();
    let mut system_entries = Vec::new();

    for (source, result) in results {
        match result {
            Ok(mut entries) => collected_entries.append(&mut entries),
            Err(err) => eprintln!("  [{}] {err}", source),
        }
    }

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
    entries.extend(merged.remaining_gitlab.iter().cloned().into_activity_entries());
    entries.extend(jira_entries.iter().cloned().into_activity_entries());
    entries.extend(outlook_entries.iter().cloned().into_activity_entries());
    entries.extend(system_entries.iter().cloned().into_activity_entries());
    entries.sort_by(|a, b| b.timestamp().cmp(a.timestamp()));

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
