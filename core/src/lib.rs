pub mod cache;
pub mod collect;
pub mod model;
pub mod ts;
pub mod utils;

use anyhow::Result;
use collect::{git, gitlab, jira, outlook, system};
use model::{ActivityEntry, ActivityLog, ActivityLogMeta, CollectOptions, CollectedActivity};
use utils::{merge_git_with_gitlab, now_iso};

pub async fn collect_activity(options: CollectOptions) -> Result<ActivityLog> {
    Ok(collect_activity_detailed(options).await?.log)
}

pub async fn collect_activity_detailed(options: CollectOptions) -> Result<CollectedActivity> {
    let git_options = options.clone();
    let outlook_options = options.clone();
    let system_options = options.clone();

    // These collectors rely on blocking process execution, so keep them off the async runtime.
    let git_task = tokio::task::spawn_blocking(move || git::collect_git(&git_options));
    let outlook_task = tokio::task::spawn_blocking(move || outlook::collect_outlook(&outlook_options));
    let system_task = tokio::task::spawn_blocking(move || system::collect_system(&system_options));

    let (git_result, gitlab_result, jira_result, outlook_result, system_result) = tokio::join!(
        async {
            match git_task.await {
                Ok(result) => result,
                Err(err) => Err(err.into()),
            }
        },
        gitlab::collect_gitlab(&options),
        jira::collect_jira(&options),
        async {
            match outlook_task.await {
                Ok(result) => result,
                Err(err) => Err(err.into()),
            }
        },
        async {
            match system_task.await {
                Ok(result) => result,
                Err(err) => Err(err.into()),
            }
        },
    );

    let git_entries = unwrap_or_log("git", git_result);
    let gitlab_entries = unwrap_or_log("gitlab", gitlab_result);
    let jira_entries = unwrap_or_log("jira", jira_result);
    let outlook_entries = unwrap_or_log("outlook", outlook_result);
    let system_entries = unwrap_or_log("system", system_result);

    // A GitLab "pushed to" event is folded into the matching git commit so the timeline does not
    // show the same change twice.
    let merged = merge_git_with_gitlab(&git_entries, &gitlab_entries);
    let merged_count = merged
        .merged_git
        .iter()
        .filter(|entry| entry.gitlab_event.is_some())
        .count();

    let mut entries = Vec::with_capacity(
        merged.merged_git.len()
            + merged.remaining_gitlab.len()
            + jira_entries.len()
            + outlook_entries.len()
            + system_entries.len(),
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

    Ok(CollectedActivity {
        git_entries: merged.merged_git,
        gitlab_entries: merged.remaining_gitlab,
        jira_entries,
        outlook_entries,
        system_entries,
        merged_count,
        log: ActivityLog {
            meta: ActivityLogMeta {
                generated: now_iso(),
                since: options.since,
                until: options.until,
            },
            entries,
        },
    })
}

fn unwrap_or_log<T>(label: &str, result: Result<Vec<T>>) -> Vec<T> {
    match result {
        Ok(entries) => entries,
        Err(err) => {
            eprintln!("  [{label}] {err}");
            Vec::new()
        }
    }
}
