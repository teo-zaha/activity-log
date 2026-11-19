use std::collections::HashSet;

use chrono::{DateTime, Duration, FixedOffset};

use crate::{
    model::{
        ActivityEntry, ActivityLog, ActivityLogMeta, CollectOptions, CollectedActivity,
        CollectedActivitySummary, ConnectionCount, ConnectionId, GitEntry, GitLabEntry, GitLabType,
        IntoActivityEntries, MergeResult, Source,
    },
    provider::provider_adapter::ConnectionEntries,
    utils::now_iso,
};

/// Combines what the connections collected so far: GitLab pushes fold into their git commits,
/// and everything is sorted newest first.
///
/// `results` holds the connections that finished, in connection order; one that failed comes
/// with no entries (its failure is logged where providers run, see `collect_progressively`).
/// `pending` holds those still being checked or collected: they are named in `meta.pending`
/// and join `summary.connections` only once they finish. `carried` holds what the previous
/// collection had for pending connections (see `carried_for`): shown and merged like the rest
/// until those connections finish, but not counted.
pub fn collect_activity_from_results(
    options: CollectOptions,
    results: Vec<ConnectionEntries>,
    pending: Vec<ConnectionId>,
    carried: Vec<ConnectionEntries>,
) -> CollectedActivity {
    let mut connections = Vec::new();
    let mut git_entries = Vec::new();
    let mut gitlab_entries = Vec::new();
    let mut entries = Vec::new();

    let carried = carried.into_iter().flat_map(|(_, entries)| entries);
    let mut finished = Vec::new();
    for (connection, connection_entries) in results {
        connections.push(connection);
        finished.extend(connection_entries);
    }
    for entry in finished.into_iter().chain(carried) {
        match entry {
            ActivityEntry::Git(entry) => git_entries.push(entry),
            ActivityEntry::GitLab(entry) => gitlab_entries.push(entry),
            other => entries.push(other),
        }
    }

    // A GitLab "pushed to" event is folded into the matching git commit so the timeline does not
    // show the same change twice.
    let merged = merge_git_with_gitlab(&git_entries, &gitlab_entries);
    let counted = |source: Source, name: &str| {
        connections
            .iter()
            .any(|connection| connection.source == source && connection.name == name)
    };
    let merged_count = merged
        .merged_git
        .iter()
        .filter(|entry| entry.gitlab_event.is_some() && counted(Source::Git, &entry.connection))
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

/// Folds each GitLab push into the git commit it pushed, so the change shows once. A push whose
/// commit is not among `git_entries` (not collected yet, another author's, no local clone)
/// stays an entry of its own. Pushes and commits pair one to one: a second push of the same
/// commit (another branch, another connection) stays an entry, and a commit two clones have
/// takes the push once. Which pairs up is decided by what the entries are, never by their
/// order, so a log rebuilt from its parts folds the same way: the earliest push (then by
/// connection and id) goes to the first commit by connection and repository.
pub(crate) fn merge_git_with_gitlab(
    git_entries: &[GitEntry],
    gitlab_entries: &[GitLabEntry],
) -> MergeResult {
    use std::collections::HashMap;

    let mut pushes = HashMap::<&str, Vec<usize>>::new();
    for (index, entry) in gitlab_entries.iter().enumerate() {
        if let Some(sha) = push_sha(entry) {
            pushes.entry(sha).or_default().push(index);
        }
    }
    let mut commits = HashMap::<&str, Vec<usize>>::new();
    for (index, entry) in git_entries.iter().enumerate() {
        if pushes.contains_key(entry.hash.as_str()) {
            commits.entry(entry.hash.as_str()).or_default().push(index);
        }
    }

    let mut folded_into = vec![None; git_entries.len()];
    let mut folded = vec![false; gitlab_entries.len()];
    for (sha, mut commit_indices) in commits {
        let mut push_indices = pushes.remove(sha).unwrap_or_default();
        push_indices.sort_by_key(|&index| {
            let push = &gitlab_entries[index];
            (
                parse_timestamp(&push.timestamp),
                push.connection.as_str(),
                push.id.as_str(),
            )
        });
        commit_indices.sort_by_key(|&index| {
            let commit = &git_entries[index];
            (commit.connection.as_str(), commit.repo.as_str())
        });
        for (commit, push) in commit_indices.into_iter().zip(push_indices) {
            folded_into[commit] = Some(push);
            folded[push] = true;
        }
    }

    let merged_git = git_entries
        .iter()
        .zip(folded_into)
        .map(|(entry, push)| {
            let mut entry = entry.clone();
            if let Some(push) = push {
                entry.gitlab_event = Some(Box::new(gitlab_entries[push].clone()));
            }
            entry
        })
        .collect::<Vec<_>>();
    let remaining_gitlab = gitlab_entries
        .iter()
        .zip(folded)
        .filter(|(_, folded)| !folded)
        .map(|(entry, _)| entry.clone())
        .collect::<Vec<_>>();

    MergeResult {
        merged_git,
        remaining_gitlab,
    }
}

/// Only GitLab push events have a commit SHA we can safely correlate with local git history.
fn push_sha(entry: &GitLabEntry) -> Option<&str> {
    (entry.entry_type == GitLabType::PushedTo)
        .then_some(entry.pushed_commit.as_deref())
        .flatten()
}

/// Numbers entries of one connection whose id an earlier one has (`id#2`, `id#3`, in order),
/// so `(source, connection, id)` names one entry even when a source repeats itself (two events
/// of a kind in one second, a feed with an event twice). Returns how many it renamed.
pub fn make_ids_unique(entries: &mut [ActivityEntry]) -> usize {
    let mut taken = entries
        .iter()
        .map(|entry| entry.id().to_string())
        .collect::<HashSet<_>>();
    let mut seen = HashSet::new();
    let mut renamed = 0;
    for entry in entries {
        if seen.insert(entry.id().to_string()) {
            continue;
        }
        let unique = (2..)
            .map(|n| format!("{}#{n}", entry.id()))
            .find(|candidate| !taken.contains(candidate))
            .expect("an unused id exists");
        taken.insert(unique.clone());
        seen.insert(unique.clone());
        *entry.id_mut() = unique;
        renamed += 1;
    }
    renamed
}

/// The entries of `log` by connection, as the connections collected them: a git commit that
/// absorbed a GitLab push gives the push back to its GitLab connection. Connections keep the
/// order in which their first entry appears.
pub fn entries_by_connection(log: &ActivityLog) -> Vec<ConnectionEntries> {
    let mut by_connection = Vec::<ConnectionEntries>::new();
    let mut push = |entry: ActivityEntry| {
        let id = ConnectionId::new(entry.source(), entry.connection());
        match by_connection.iter_mut().find(|(other, _)| *other == id) {
            Some((_, entries)) => entries.push(entry),
            None => by_connection.push((id, vec![entry])),
        }
    };
    for entry in &log.entries {
        match entry {
            ActivityEntry::Git(commit) if commit.gitlab_event.is_some() => {
                let mut commit = commit.clone();
                let push_event = commit.gitlab_event.take().expect("checked above");
                push(ActivityEntry::Git(commit));
                push(ActivityEntry::GitLab(*push_event));
            }
            other => push(other.clone()),
        }
    }
    by_connection
}

/// What a collection of `options` carries from `shown`, the log on screen when it starts, for
/// its connections that are not in yet. Collecting the same range again keeps everything:
/// meetings are collected by overlap, so one that began before the range is still in it. For
/// another range, an entry stays if it falls inside; a meeting if it overlaps.
pub fn carry_over(shown: &ActivityLog, options: CollectOptions) -> Vec<ConnectionEntries> {
    let same_range = parse_timestamp(&shown.meta.start).map(|at| at.to_utc())
        == Some(options.start)
        && parse_timestamp(&shown.meta.end).map(|at| at.to_utc()) == Some(options.end);
    let mut carried = entries_by_connection(shown);
    if !same_range {
        for (_, entries) in &mut carried {
            entries.retain(|entry| within(entry, options));
        }
        carried.retain(|(_, entries)| !entries.is_empty());
    }
    carried
}

fn within(entry: &ActivityEntry, options: CollectOptions) -> bool {
    let Some(start) = parse_timestamp(entry.timestamp()).map(|at| at.to_utc()) else {
        return false;
    };
    let minutes = match entry {
        ActivityEntry::Outlook(meeting) => meeting.duration_minutes,
        ActivityEntry::Calendar(meeting) => meeting.duration_minutes,
        _ => 0,
    };
    options.overlaps(start, start + Duration::minutes(i64::from(minutes.max(0))))
}

/// The part of `carried` to show while `pending` connections are not in: only theirs, without
/// the commits a connection in `finished` already has (two clones or locations can hold the
/// same commit, and Git lists it once) and without a commit carried twice.
pub fn carried_for(
    carried: &[ConnectionEntries],
    pending: &[ConnectionId],
    finished: &[ConnectionEntries],
) -> Vec<ConnectionEntries> {
    let mut commits = finished
        .iter()
        .flat_map(|(_, entries)| entries)
        .filter_map(|entry| match entry {
            ActivityEntry::Git(commit) => Some(commit.hash.clone()),
            _ => None,
        })
        .collect::<HashSet<_>>();
    carried
        .iter()
        .filter(|(id, _)| pending.contains(id))
        .map(|(id, entries)| {
            let entries = entries
                .iter()
                .filter(|entry| match entry {
                    ActivityEntry::Git(commit) => commits.insert(commit.hash.clone()),
                    _ => true,
                })
                .cloned()
                .collect();
            (id.clone(), entries)
        })
        .collect()
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
