//! What became of each connection in a collection, as the terminal says it, and whether the
//! run failed: decided from the run's typed report and each check's problem, never from text.

use activity_log_core::{
    model::{CollectedActivitySummary, ProviderInfo, ProviderProblem},
    runtime::report::{CollectFailure, Outcome, RunReport, Slot},
};

#[cfg(test)]
mod tests;

/// One connection's result in a collection.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ConnectionResult<'a> {
    Collected {
        info: &'a ProviderInfo,
        entries: usize,
    },
    /// Its check found it unusable; its `problem` says whether that fails the run.
    Unusable { info: &'a ProviderInfo },
    /// It was checked, and its collection failed.
    Failed {
        info: &'a ProviderInfo,
        failure: &'a CollectFailure,
    },
    /// The run ended before it settled.
    Unfinished { info: &'a ProviderInfo },
}

/// What a problem means for a run of the CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Effect {
    /// The source is configured but cannot work: the run fails.
    Fails,
    /// The source has nothing to give here (another OS, an app not installed or not running,
    /// nothing to read): said, and the run goes on.
    Skips,
}

/// The one place a problem gets its words in the terminal and its effect on the run.
pub(crate) fn problem_terms(problem: ProviderProblem) -> (&'static str, Effect) {
    match problem {
        ProviderProblem::TimedOut => ("timed out", Effect::Fails),
        ProviderProblem::Refused => ("credentials refused", Effect::Fails),
        ProviderProblem::ScopeMissing => ("token scope missing", Effect::Fails),
        ProviderProblem::FeedUnavailable => ("calendar link unavailable", Effect::Fails),
        ProviderProblem::Throttled => ("server busy", Effect::Fails),
        ProviderProblem::Unreachable => ("server unreachable", Effect::Fails),
        ProviderProblem::PermissionDenied => ("permission denied", Effect::Fails),
        ProviderProblem::NotConfigured => ("not configured", Effect::Fails),
        ProviderProblem::Failed => ("check failed", Effect::Fails),
        ProviderProblem::CollectionFailed => ("collection failed", Effect::Fails),
        ProviderProblem::SecretUnavailable => ("no key from the secure store", Effect::Fails),
        ProviderProblem::Unsupported => ("not supported on this OS", Effect::Skips),
        ProviderProblem::NotInstalled => ("not installed", Effect::Skips),
        ProviderProblem::NotRunning => ("not running", Effect::Skips),
        ProviderProblem::NoData => ("nothing to read", Effect::Skips),
    }
}

/// Words for a check without a problem, which an unusable connection should never have.
const UNAVAILABLE: &str = "unavailable";

impl ConnectionResult<'_> {
    /// Whether this connection makes the run fail: a collection that failed or did not end
    /// always does, a check by its problem's effect.
    pub(crate) fn fails_run(&self) -> bool {
        match self {
            Self::Collected { .. } => false,
            Self::Unusable { info } => info
                .problem
                .is_none_or(|problem| problem_terms(problem).1 == Effect::Fails),
            Self::Failed { .. } | Self::Unfinished { .. } => true,
        }
    }

    /// The line the terminal shows.
    pub(crate) fn line(&self) -> String {
        let name = |info: &ProviderInfo| format!("{} ({})", info.connection, info.label);
        match self {
            Self::Collected { info, entries } => {
                format!("  ✓ {} ({entries} entries)", name(info))
            }
            Self::Unusable { info } => {
                let (words, effect) = info
                    .problem
                    .map_or((UNAVAILABLE, Effect::Fails), problem_terms);
                let mark = match effect {
                    Effect::Fails => "✗",
                    Effect::Skips => "-",
                };
                match &info.reason {
                    Some(reason) => format!("  {mark} {} — {words}: {reason}", name(info)),
                    None => format!("  {mark} {} — {words}", name(info)),
                }
            }
            Self::Failed { info, failure } => format!(
                "  ✗ {} — {}: {}",
                name(info),
                problem_terms(failure.problem()).0,
                failure.message()
            ),
            Self::Unfinished { info } => format!("  ✗ {} — did not finish", name(info)),
        }
    }
}

/// Each checked connection's result, in connection order: `providers` are the checks the run
/// collected from, `report` its final report, `summary` the counts of its log.
pub(crate) fn connection_results<'a>(
    providers: &'a [ProviderInfo],
    report: &'a RunReport,
    summary: &CollectedActivitySummary,
) -> Vec<ConnectionResult<'a>> {
    providers
        .iter()
        .map(|info| {
            let slot = report
                .slots
                .iter()
                .find(|(id, _)| *id == info.connection)
                .map(|(_, slot)| slot);
            match slot {
                Some(Slot::Settled(outcome) | Slot::Kept(outcome)) => match outcome {
                    Outcome::Collected(_) => ConnectionResult::Collected {
                        info,
                        entries: summary.entry_count_for(&info.connection),
                    },
                    Outcome::Failed { failure, .. } => ConnectionResult::Failed { info, failure },
                },
                Some(Slot::Skipped) => ConnectionResult::Unusable { info },
                Some(Slot::Pending { .. }) | None => ConnectionResult::Unfinished { info },
            }
        })
        .collect()
}
