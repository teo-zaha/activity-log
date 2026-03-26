use crate::{
    model::{
        ActivityEntry, ActivityLog, ActivityLogMeta, CollectOptions, CollectedActivity,
        CollectedActivitySummary,
    },
    provider::provider_collection::ProviderCollection,
    utils::{merge_git_with_gitlab, now_iso},
};

pub fn collect_activity_from_results(
    options: CollectOptions,
    results: Vec<(crate::model::Source, anyhow::Result<ProviderCollection>)>,
) -> CollectedActivity {
    let mut git_entries = Vec::new();
    let mut gitlab_entries = Vec::new();
    let mut jira_entries = Vec::new();
    let mut outlook_entries = Vec::new();
    let mut system_entries = Vec::new();

    for (source, result) in results {
        match result {
            Ok(ProviderCollection::Git(entries)) => git_entries.extend(entries),
            Ok(ProviderCollection::GitLab(entries)) => gitlab_entries.extend(entries),
            Ok(ProviderCollection::Jira(entries)) => jira_entries.extend(entries),
            Ok(ProviderCollection::Outlook(entries)) => outlook_entries.extend(entries),
            Ok(ProviderCollection::System(entries)) => system_entries.extend(entries),
            Err(err) => eprintln!("  [{}] {err}", source),
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

    entries.extend(merged.merged_git.iter().cloned().map(ActivityEntry::Git));
    entries.extend(
        merged
            .remaining_gitlab
            .iter()
            .cloned()
            .map(ActivityEntry::GitLab),
    );
    entries.extend(jira_entries.iter().cloned().map(ActivityEntry::Jira));
    entries.extend(outlook_entries.iter().cloned().map(ActivityEntry::Outlook));
    entries.extend(system_entries.iter().cloned().map(ActivityEntry::System));
    entries.sort_by(|a, b| b.timestamp().cmp(a.timestamp()));

    CollectedActivity {
        summary: summary.clone(),
        log: ActivityLog {
            meta: ActivityLogMeta {
                generated: now_iso(),
                since: options.since,
                until: options.until,
            },
            summary,
            entries,
        },
    }
}
