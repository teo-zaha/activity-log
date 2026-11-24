//! What a collection run reports: each connection's own entries, before the Git ⇄ GitLab
//! fold, so a host can keep, carry and evict them per connection and fold the log from them.

use std::{sync::Arc, time::Duration};

use futures::future::BoxFuture;

use crate::{
    aggregator::{carried_for, collect_activity_from_results},
    model::{ActivityEntry, CollectOptions, CollectedActivity, ConnectionId, ProviderProblem},
    provider::{ProviderRuntime, source_failure::problem_of},
};

pub use super::keys::CollectionKey;

/// Entries a connection produced under one collection key.
#[derive(Debug, Clone)]
pub struct Stored {
    pub key: CollectionKey,
    /// After the kind's `finish_kind` and `make_ids_unique`, before the Git ⇄ GitLab fold.
    pub entries: Arc<[ActivityEntry]>,
}

/// How a connection's last collection ended.
#[derive(Debug, Clone)]
pub enum Outcome {
    Collected(Stored),
    /// Listed with no entries and marked degraded.
    Failed {
        key: CollectionKey,
        failure: CollectFailure,
    },
}

/// How a connection's collection failed. The problem tells a busy server, a refusal, a
/// timeout and an unreachable server apart from other failures. The message is for people.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectFailure {
    problem: ProviderProblem,
    message: String,
}

impl CollectFailure {
    /// `err` as its connection shows it: with the problem a typed cause in its chain names
    /// (`source_failure::problem_of`), else `collectionFailed`.
    pub fn of(err: &anyhow::Error) -> Self {
        let problem = problem_of(err).unwrap_or(ProviderProblem::CollectionFailed);
        Self {
            problem,
            message: format!("{err:#}"),
        }
    }

    /// A collection that did not end in `limit`.
    pub fn timed_out(limit: Duration) -> Self {
        Self {
            problem: ProviderProblem::TimedOut,
            message: format!("Collection timed out after {}s", limit.as_secs()),
        }
    }

    /// The problem a typed cause named (`timedOut`, `refused`, `throttled`, `notRunning`, …), else
    /// `collectionFailed`.
    pub fn problem(&self) -> ProviderProblem {
        self.problem
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl Outcome {
    pub fn key(&self) -> CollectionKey {
        match self {
            Self::Collected(stored) => stored.key,
            Self::Failed { key, .. } => *key,
        }
    }
}

/// One connection's place in a run.
#[derive(Debug, Clone)]
pub enum Slot {
    /// Its check or collection is still running. `carried` is what the log shown when the run
    /// started had for it, with the key it was collected under.
    Pending { carried: Option<Stored> },
    /// Its check found it unusable: not listed.
    Skipped,
    /// Settled in this run. While siblings of its kind are pending, `Collected` holds the kind's
    /// provisional `finish_kind` output, which a later report replaces.
    Settled(Outcome),
    /// Not collected by this run: the outcome from the store the run started from.
    Kept(Outcome),
}

/// Every connection in the run's plan, in connection order. Each report is complete, never a
/// delta.
#[derive(Debug, Clone, Default)]
pub struct RunReport {
    pub slots: Vec<(ConnectionId, Slot)>,
}

impl RunReport {
    /// The log clients see: finished connections in order, a failed one listed without
    /// entries; pending ones named in `meta.pending` and shown with what they carry (not
    /// counted, a commit a finished connection has shown once); skipped ones left out. GitLab
    /// pushes fold into their commits here, from each connection's own entries.
    pub fn project(&self, options: CollectOptions) -> CollectedActivity {
        let mut results = Vec::new();
        let mut pending = Vec::new();
        let mut carried = Vec::new();
        for (id, slot) in &self.slots {
            match slot {
                Slot::Pending { carried: shown } => {
                    pending.push(id.clone());
                    if let Some(shown) = shown {
                        carried.push((id, &shown.entries[..]));
                    }
                }
                Slot::Skipped => {}
                Slot::Settled(outcome) | Slot::Kept(outcome) => {
                    let entries = match outcome {
                        Outcome::Collected(stored) => stored.entries.to_vec(),
                        Outcome::Failed { .. } => Vec::new(),
                    };
                    results.push((id.clone(), entries));
                }
            }
        }
        let carried = carried_for(carried, &pending, &results);
        collect_activity_from_results(options, results, pending, carried)
    }
}

/// What a run does with one connection.
pub enum Plan {
    /// Collect it once its recorded check yields a usable runtime.
    Collect {
        check: BoxFuture<'static, Result<Checked, Superseded>>,
        carried: Option<Stored>,
    },
    Keep(Outcome),
}

/// A recorded check: the runtime with the collection key it was recorded under.
#[derive(Clone)]
pub struct Checked {
    pub runtime: ProviderRuntime,
    pub key: CollectionKey,
}

/// The check a plan waited for was replaced, or its connection removed.
#[derive(Debug, Clone, Copy)]
pub struct Superseded;

pub type Progress = Arc<dyn Fn(RunReport) + Send + Sync>;

/// Collects a run; the final report is the result. A run does not fail as a whole: each
/// connection's failure is its own `Outcome::Failed`, and only a panic ends a run failed.
/// Production: [`crate::collect_planned`]; tests: fakes.
pub type Collector = Arc<
    dyn Fn(CollectOptions, Vec<(ConnectionId, Plan)>, Progress) -> BoxFuture<'static, RunReport>
        + Send
        + Sync,
>;
