use chrono::{DateTime, FixedOffset};

use crate::{
    model::{
        ActivityEntry, ActivityLog, ActivityLogMeta, CollectOptions, CollectedActivity,
        CollectedActivitySummary, ConnectionCount, ConnectionId, IntoActivityEntries,
    },
    provider::provider_adapter::ConnectionEntries,
    utils::{merge_git_with_gitlab, now_iso},
};

/// Combines what the connections collected so far: GitLab pushes fold into their git commits,
/// and everything is sorted newest first.
///
/// `results` holds the connections that finished, in connection order; one that failed comes
/// with no entries (its failure is logged where providers run, see `collect_progressively`).
/// `pending` holds those still being checked or collected: they are named in `meta.pending`
/// and join `summary.connections` only once they finish.
pub fn collect_activity_from_results(
    options: CollectOptions,
    results: Vec<ConnectionEntries>,
    pending: Vec<ConnectionId>,
) -> CollectedActivity {
    let mut connections = Vec::new();
    let mut git_entries = Vec::new();
    let mut gitlab_entries = Vec::new();
    let mut entries = Vec::new();

    for (connection, connection_entries) in results {
        connections.push(connection);
        for entry in connection_entries {
            match entry {
                ActivityEntry::Git(entry) => git_entries.push(entry),
                ActivityEntry::GitLab(entry) => gitlab_entries.push(entry),
                other => entries.push(other),
            }
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
    entries.extend(merged.merged_git.into_iter().into_activity_entries());
    entries.extend(merged.remaining_gitlab.into_iter().into_activity_entries());
    entries.sort_by(|a, b| compare_timestamps_desc(a.timestamp(), b.timestamp()));

    let summary = CollectedActivitySummary {
        connections: connections
            .into_iter()
            .map(|connection| ConnectionCount {
                count: entries
                    .iter()
                    .filter(|entry| {
                        entry.source() == connection.source && entry.connection() == connection.name
                    })
                    .count(),
                connection,
            })
            .collect(),
        merged_count,
    };

    CollectedActivity {
        summary: summary.clone(),
        log: ActivityLog {
            meta: ActivityLogMeta {
                generated: now_iso(),
                start: options.start.to_rfc3339(),
                end: options.end.to_rfc3339(),
                pending,
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
