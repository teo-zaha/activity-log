use anyhow::Context;

use super::{ClassifiedProblem, SourceFailure, SourceProblem, problem_of};
use crate::{
    model::ProviderProblem,
    provider::remote_error::{ApiError, RemoteFailure},
};

#[test]
fn the_first_typed_cause_names_the_problem() {
    let closed = anyhow::Error::new(SourceFailure::new(
        SourceProblem::NotRunning,
        "Outlook is not running",
    ))
    .context("Reading meetings");
    assert_eq!(
        problem_of(&closed),
        Some(ClassifiedProblem::plain(ProviderProblem::NotRunning))
    );

    let refused = anyhow::Error::new(RemoteFailure {
        service: "Jira".into(),
        error: ApiError::Timeout,
    });
    assert_eq!(
        problem_of(&refused),
        Some(ClassifiedProblem::plain(ProviderProblem::TimedOut))
    );

    let plain = Err::<(), _>(anyhow::anyhow!("disk full"))
        .context("Reading the feed")
        .unwrap_err();
    assert_eq!(problem_of(&plain), None);
}

/// A store that gave no key is `secretUnavailable` whatever kind of store error it was, also
/// one that ran out of time: never `timedOut`, which is a slow server.
#[test]
fn a_store_that_gave_no_key_is_secret_unavailable() {
    use crate::secrets::{StoreError, StoreErrorKind};
    for kind in [
        StoreErrorKind::Unavailable,
        StoreErrorKind::Refused,
        StoreErrorKind::Failed,
    ] {
        let err = anyhow::Error::new(StoreError::new(
            kind,
            anyhow::anyhow!("did not answer within 120 s"),
        ))
        .context("Opening the token");
        assert_eq!(
            problem_of(&err),
            Some(ClassifiedProblem::plain(ProviderProblem::SecretUnavailable))
        );
    }
}

#[cfg(target_os = "macos")]
#[test]
fn generic_applescript_not_running_does_not_name_an_app() {
    use super::FixCause;
    use crate::platform::macos::AppleScriptError;

    for (number, expected) in [
        (-1743, ClassifiedProblem::caused(FixCause::AutomationDenied)),
        (-600, ClassifiedProblem::plain(ProviderProblem::NotRunning)),
        (-609, ClassifiedProblem::plain(ProviderProblem::NotRunning)),
    ] {
        let err = AppleScriptError {
            code: Some(number),
            message: "Example failure".into(),
        };
        assert_eq!(problem_of(&err.into()), Some(expected));
    }
}
