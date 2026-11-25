use std::collections::{HashMap, HashSet};

use chrono::Duration;

use crate::{
    model::{
        ActivityEntry, ActivityLog, ActivityLogMeta, CollectOptions, CollectedActivity,
        CollectedActivitySummary, ConnectionCount, ConnectionId, GitEntry, IntoActivityEntries,
        Source, activity::hosted_commit::HostedCommit,
    },
    provider::provider_adapter::ConnectionEntries,
    utils::now_iso,
};

/// Combines what the connections collected so far: host events fold into their Git commits,
/// and everything is sorted newest first.
///
/// `results` holds the connections that finished, in connection order, with newly collected or
/// retained entries. A failure is logged where providers run, see `collect_progressively`.
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
    let mut host_entries = Vec::new();
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
            other if HostedCommit::is_host_event(&other) => host_entries.push(other),
            other => entries.push(other),
        }
    }

    let (git_entries, host_entries) = fold_hosted_commits(git_entries, host_entries);
    let counted = |source: Source, name: &str| {
        connections
            .iter()
            .any(|connection| connection.source == source && connection.name == name)
    };
    let merged_count = git_entries
        .iter()
        .filter(|entry| {
            entry.has_host_event()
                && entry
                    .connection
                    .as_str()
                    .is_some_and(|name| counted(Source::Git, name))
        })
        .count();
    entries.extend(git_entries.into_iter().into_activity_entries());
    entries.extend(host_entries);
    entries.sort_by(|a, b| b.timestamp().cmp(a.timestamp()));

    let mut counts = HashMap::<(Source, &str), usize>::new();
    for entry in &entries {
        if let Some(name) = entry.connection() {
            *counts.entry((entry.source(), name)).or_default() += 1;
        }
    }
    let connections = connections
        .into_iter()
        .map(|connection| ConnectionCount {
            count: counts
                .get(&(connection.source.clone(), connection.name.as_str()))
                .copied()
                .unwrap_or_default(),
            connection,
        })
        .collect();
    let summary = CollectedActivitySummary {
        connections,
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

/// For each SHA, pair each host's ordered events with the same ordered local Git rows.
fn fold_hosted_commits(
    mut git_entries: Vec<GitEntry>,
    host_entries: Vec<ActivityEntry>,
) -> (Vec<GitEntry>, Vec<ActivityEntry>) {
    let mut hosts_by_sha = HashMap::<&str, HashMap<Source, Vec<(usize, HostedCommit<'_>)>>>::new();
    for (index, entry) in host_entries.iter().enumerate() {
        if let Some(candidate) = HostedCommit::of(entry) {
            hosts_by_sha
                .entry(candidate.sha())
                .or_default()
                .entry(candidate.source())
                .or_default()
                .push((index, candidate));
        }
    }

    let mut rows_by_sha = HashMap::<&str, Vec<usize>>::new();
    for (index, entry) in git_entries.iter().enumerate() {
        if hosts_by_sha.contains_key(entry.hash.as_str()) {
            rows_by_sha.entry(&entry.hash).or_default().push(index);
        }
    }
    let mut assignments = Vec::new();
    for (sha, hosts) in hosts_by_sha {
        let Some(mut rows) = rows_by_sha.remove(sha) else {
            continue;
        };
        rows.sort_by_key(|&index| clone_key(&git_entries[index]));
        for (_, mut candidates) in hosts {
            candidates.sort_by(|(_, left), (_, right)| left.order_key().cmp(&right.order_key()));
            assignments.extend(rows.iter().copied().zip(candidates));
        }
    }

    let mut attached = vec![false; host_entries.len()];
    for (target, (index, candidate)) in assignments {
        candidate.attach_to(&mut git_entries[target]);
        attached[index] = true;
    }
    let remaining = host_entries
        .into_iter()
        .enumerate()
        .filter_map(|(index, entry)| (!attached[index]).then_some(entry))
        .collect();
    (git_entries, remaining)
}

fn clone_key(entry: &GitEntry) -> (Option<&str>, &str) {
    (entry.connection.as_str(), &entry.repo)
}

/// Numbers entries of one connection whose id an earlier one has (`id#2`, `id#3`, in order),
/// so `(source, connection, id)` names one entry even when a source repeats itself (two events
/// of a kind in one second, a feed with an event twice). Returns how many it renamed.
pub fn make_ids_unique(entries: &mut [ActivityEntry]) -> usize {
    let mut taken = entries
        .iter()
        .map(|entry| entry.id().to_string())
        .collect::<HashSet<_>>();
    if taken.len() == entries.len() {
        return 0;
    }
    let mut seen = HashSet::new();
    // The next number to try per repeated id: every `id#n` below it is taken, and `taken`
    // only grows, so starting there finds what counting from 2 would.
    let mut next = HashMap::<String, usize>::new();
    let mut renamed = 0;
    for entry in entries {
        if seen.insert(entry.id().to_string()) {
            continue;
        }
        let n = next.entry(entry.id().to_string()).or_insert(2);
        let unique = loop {
            let candidate = format!("{}#{n}", entry.id());
            *n += 1;
            if !taken.contains(&candidate) {
                break candidate;
            }
        };
        taken.insert(unique.clone());
        seen.insert(unique.clone());
        *entry.id_mut() = unique;
        renamed += 1;
    }
    renamed
}

/// Whether a carried entry belongs to a collection of `options`: an entry if it falls inside,
/// a meeting if it overlaps.
pub(crate) fn within(entry: &ActivityEntry, options: CollectOptions) -> bool {
    let start = entry.timestamp().utc();
    let minutes = entry
        .meeting()
        .map_or(0, |meeting| meeting.duration_minutes);
    options.overlaps(start, start + Duration::minutes(i64::from(minutes.max(0))))
}

/// The part of `carried` to show while `pending` connections are not in: only theirs, without
/// the commits a connection in `finished` already has (two clones or locations can hold the
/// same commit, and Git lists it once) and without a commit carried twice.
pub fn carried_for<'a>(
    carried: impl IntoIterator<Item = (&'a ConnectionId, &'a [ActivityEntry])>,
    pending: &[ConnectionId],
    finished: &[ConnectionEntries],
) -> Vec<ConnectionEntries> {
    let mut commits = finished
        .iter()
        .flat_map(|(_, entries)| entries)
        .filter_map(|entry| match entry {
            ActivityEntry::Git(commit) => Some(commit.hash.as_str()),
            _ => None,
        })
        .collect::<HashSet<_>>();
    carried
        .into_iter()
        .filter(|(id, _)| pending.contains(id))
        .map(|(id, entries)| {
            let entries = entries
                .iter()
                .filter(|entry| match entry {
                    ActivityEntry::Git(commit) => commits.insert(commit.hash.as_str()),
                    _ => true,
                })
                .cloned()
                .collect();
            (id.clone(), entries)
        })
        .collect()
}

#[cfg(test)]
mod tests;
