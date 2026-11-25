//! GitHub's current pull-request stacks, using native membership ahead of the branch walk.

mod lookup;
mod native;

use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, Utc};
use futures::{StreamExt, stream};

use crate::{
    collect::change_stack::{
        Budget, ChangeRef, UnreadChanges, change_stacks, change_stacks_with_seeds,
    },
    diagnostics,
    model::{ActivityEntry, ChangeStack},
    provider::{
        github::{GitHubConfig, api::GitHubApi},
        provider_context::ProviderContext,
    },
};

use lookup::{AliasFields, CONCURRENCY, GitHubChanges, GitHubRepo, PULLS_PER_REQUEST};
use native::{NativePull, NativeResult};

type Pull = ChangeRef<GitHubRepo>;
type Wanted = (Pull, DateTime<Utc>);
type WantedNumber = (u64, DateTime<Utc>);

/// Replaces annotations in one pass once lookups are settled. A stopped annotation leaves
/// the collected entries available.
pub(super) async fn annotate(entries: &mut [ActivityEntry], context: ProviderContext) {
    let wanted: Vec<_> = entries
        .iter_mut()
        .filter_map(|entry| {
            let ActivityEntry::GitHub(entry) = entry else {
                return None;
            };
            let pull = entry.action.pull_request_mut()?;
            Some((
                change_ref(&entry.repository, pull.number)?,
                entry.timestamp.utc(),
            ))
        })
        .collect();
    let stacks = if wanted.is_empty() {
        HashMap::new()
    } else {
        look_up(wanted, &context).await
    };
    for entry in entries {
        if let ActivityEntry::GitHub(entry) = entry
            && let Some(pull) = entry.action.pull_request_mut()
        {
            pull.change_stack = change_ref(&entry.repository, pull.number)
                .and_then(|change| stacks.get(&change).cloned());
        }
    }
}

fn change_ref(repository: &str, number: u64) -> Option<Pull> {
    Some(ChangeRef {
        repo: GitHubRepo::parse(repository)?,
        number,
    })
}

async fn look_up(wanted: Vec<Wanted>, context: &ProviderContext) -> HashMap<Pull, ChangeStack> {
    let config = match context.config::<GitHubConfig>() {
        Ok(config) => config,
        Err(error) => {
            diagnostics::warn("github", format!("Change stacks skipped: {error:#}"));
            return HashMap::new();
        }
    };
    let api = match GitHubApi::new(config) {
        Ok(api) => api,
        Err(error) => {
            diagnostics::warn(
                "github",
                format!(
                    "Change stacks skipped: {}",
                    error.reason(GitHubApi::SERVICE)
                ),
            );
            return HashMap::new();
        }
    };
    let lookup = GitHubChanges::new(&api);
    let stacks = native_then_walk(wanted, &lookup, &Budget::for_collection()).await;
    lookup.log_observations();
    stacks
}

async fn native_then_walk(
    wanted: Vec<Wanted>,
    lookup: &GitHubChanges<'_>,
    budget: &Budget,
) -> HashMap<Pull, ChangeStack> {
    let ordered = by_repo(&wanted);
    let mut stacks = HashMap::new();
    let mut seeds = HashMap::new();
    let mut fallback = Vec::new();
    let mut unread = Vec::new();
    let mut queries = Vec::new();
    for (repo, requests) in &ordered {
        for batch in requests.chunks(PULLS_PER_REQUEST) {
            queries.push(async move {
                let numbers: Vec<_> = batch.iter().map(|(number, _)| *number).collect();
                let rows = budget
                    .run(lookup.aliases::<NativePull>(repo, &numbers, AliasFields::Native, budget))
                    .await;
                (repo, batch, rows)
            });
        }
    }
    let unsupported = {
        // Probe the schema before filling the request window on a server that lacks it.
        let mut queries = queries.into_iter();
        let first = stream::iter(queries.next()).buffered(1);
        let rest = stream::iter(queries).buffered(CONCURRENCY);
        let mut answers = std::pin::pin!(first.chain(rest));
        loop {
            let Some((repo, batch, rows)) = answers.next().await else {
                break false;
            };
            let mut failed = |failure, count| {
                unread.push(UnreadChanges {
                    repo: repo.clone(),
                    failure,
                    count,
                });
            };
            match rows {
                Err(failure) if native::unsupported(&failure) => break true,
                Err(failure) => failed(failure, batch.len()),
                Ok(rows) => {
                    for (&(number, at), row) in batch.iter().zip(rows) {
                        let change = ChangeRef {
                            repo: repo.clone(),
                            number,
                        };
                        let row = match row {
                            Ok(Some(row)) => row,
                            Ok(None) => continue,
                            Err(failure) => {
                                failed(failure, 1);
                                continue;
                            }
                        };
                        match row.stack(number, lookup) {
                            Ok(NativeResult::Walk) => fallback.push((change, at)),
                            Ok(NativeResult::Seed(seed)) => {
                                seeds.insert(change.clone(), seed);
                                fallback.push((change, at));
                            }
                            Ok(NativeResult::Resolved(Some(stack))) => {
                                stacks.insert(change, stack);
                            }
                            Ok(NativeResult::Resolved(None)) => {}
                            Err(failure) => failed(failure, 1),
                        }
                    }
                }
            }
        }
    };
    if unsupported {
        let report = change_stacks(wanted, lookup, budget).await;
        report.log("github", GitHubApi::SERVICE);
        return report.stacks;
    }
    let mut report = change_stacks_with_seeds(fallback, &seeds, unread, lookup, budget).await;
    report.stacks.extend(stacks);
    report.log("github", GitHubApi::SERVICE);
    report.stacks
}

fn by_repo(wanted: &[Wanted]) -> Vec<(GitHubRepo, Vec<WantedNumber>)> {
    let mut grouped = BTreeMap::<GitHubRepo, BTreeMap<u64, DateTime<Utc>>>::new();
    for (change, at) in wanted {
        grouped
            .entry(change.repo.clone())
            .or_default()
            .entry(change.number)
            .and_modify(|last| *last = (*last).max(*at))
            .or_insert(*at);
    }
    let mut ordered: Vec<_> = grouped
        .into_iter()
        .map(|(repo, numbers)| {
            let mut numbers: Vec<_> = numbers.into_iter().collect();
            numbers.sort_unstable_by_key(|&(number, at)| (std::cmp::Reverse(at), number));
            (repo, numbers)
        })
        .collect();
    ordered.sort_unstable_by(|(left, ls), (right, rs)| {
        rs[0].1.cmp(&ls[0].1).then_with(|| left.cmp(right))
    });
    ordered
}

#[cfg(test)]
mod tests;
