use std::time::Duration;

use activity_log_core::{
    model::{
        CollectedActivitySummary, ConnectionCount, ConnectionId, ProviderInfo, ProviderProblem,
        ProviderStatus, Source,
    },
    runtime::report::{CollectFailure, CollectionKey, Outcome, RunReport, Slot, Stored},
};

use super::{ConnectionResult, Effect, connection_results, problem_terms};

fn info(source: Source, name: &str, problem: Option<ProviderProblem>) -> ProviderInfo {
    ProviderInfo {
        connection: ConnectionId::new(source, name),
        label: name.to_string(),
        wsl_distro: None,
        status: match problem {
            Some(_) => ProviderStatus::Unavailable,
            None => ProviderStatus::Available,
        },
        usable: problem.is_none(),
        supported_os: Vec::new(),
        reason: problem.map(|_| "why".to_string()),
        problem,
        recheckable: false,
        checked_at: "2026-01-01T00:00:00Z".to_string(),
    }
}

/// A source configured but not working fails the run; one with nothing to give here does not.
#[test]
fn a_problem_fails_the_run_only_where_the_source_cannot_work() {
    let skips = [
        ProviderProblem::Unsupported,
        ProviderProblem::NotInstalled,
        ProviderProblem::NotRunning,
        ProviderProblem::NoData,
    ];
    let fails = [
        ProviderProblem::TimedOut,
        ProviderProblem::Refused,
        ProviderProblem::Throttled,
        ProviderProblem::Unreachable,
        ProviderProblem::PermissionDenied,
        ProviderProblem::NotConfigured,
        ProviderProblem::Failed,
        ProviderProblem::CollectionFailed,
    ];
    for problem in skips {
        assert_eq!(problem_terms(problem).1, Effect::Skips, "{problem:?}");
        let info = info(Source::Outlook, "mail", Some(problem));
        let skipped = ConnectionResult::Unusable { info: &info };
        assert!(!skipped.fails_run(), "{problem:?}");
    }
    for problem in fails {
        assert_eq!(problem_terms(problem).1, Effect::Fails, "{problem:?}");
        let info = info(Source::Gitlab, "work", Some(problem));
        let refused = ConnectionResult::Unusable { info: &info };
        assert!(refused.fails_run(), "{problem:?}");
    }
}

/// A collection that ran and failed fails the run, whatever its problem.
#[test]
fn a_failed_collection_always_fails_the_run() {
    let info = info(Source::Outlook, "mail", None);
    let failure = CollectFailure::timed_out(Duration::from_secs(1));
    assert!(
        ConnectionResult::Failed {
            info: &info,
            failure: &failure,
        }
        .fails_run()
    );
    assert!(ConnectionResult::Unfinished { info: &info }.fails_run());
    assert!(
        ConnectionResult::Unusable { info: &info }.fails_run(),
        "an unusable source without a problem is not known to be fine"
    );
    assert!(
        !ConnectionResult::Collected {
            info: &info,
            entries: 0,
        }
        .fails_run()
    );
}

/// Each connection's result comes from its slot in the run and its check, in connection order,
/// with its own count.
#[test]
fn each_connection_gets_its_own_result() {
    let git = info(Source::Git, "laptop", None);
    let outlook = info(Source::Outlook, "mail", Some(ProviderProblem::Unsupported));
    let jira = info(Source::Jira, "tickets", None);
    let failure = CollectFailure::timed_out(Duration::from_secs(1));
    let report = RunReport {
        slots: vec![
            (
                jira.connection.clone(),
                Slot::Settled(Outcome::Failed {
                    key: CollectionKey::UNKEYED,
                    failure: failure.clone(),
                }),
            ),
            (outlook.connection.clone(), Slot::Skipped),
            (
                git.connection.clone(),
                Slot::Settled(Outcome::Collected(Stored {
                    key: CollectionKey::UNKEYED,
                    entries: Vec::new().into(),
                })),
            ),
        ],
    };
    let summary = CollectedActivitySummary {
        connections: vec![ConnectionCount {
            connection: git.connection.clone(),
            count: 7,
        }],
        merged_count: 0,
    };
    let providers = [git.clone(), outlook.clone(), jira.clone()];

    let results = connection_results(&providers, &report, &summary);

    assert_eq!(
        results,
        [
            ConnectionResult::Collected {
                info: &git,
                entries: 7
            },
            ConnectionResult::Unusable { info: &outlook },
            ConnectionResult::Failed {
                info: &jira,
                failure: &failure
            },
        ]
    );
}
