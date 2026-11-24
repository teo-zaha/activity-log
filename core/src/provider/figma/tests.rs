use crate::{
    model::{IdentityEvidence, RemoteIdentity},
    provider::{
        provider_check_result::CheckOutcome,
        remote_api::test_server::{Reply, TestServer},
    },
};

use super::*;

#[tokio::test]
async fn invalid_scope_is_not_configured_before_any_request() {
    let server = TestServer::start(|_| Reply::json(200, r#"{"id":"1001","handle":"ada"}"#));
    for files in [
        None,
        Some(vec!["https://other.example.org/design/key".into()]),
    ] {
        let config = FigmaConfig {
            token: "token".into(),
            files,
        };
        let result = check(&config, |config| FigmaApi::at(&server.url, config)).await;
        assert_eq!(
            result.outcome.problem(),
            Some(ProviderProblem::NotConfigured)
        );
        assert_eq!(result.identity, IdentityEvidence::None);
    }
    assert!(server.requests().is_empty());
}

#[tokio::test]
async fn startup_check_keeps_the_identity_evidence_from_figma_me() {
    let config = FigmaConfig {
        token: "token".into(),
        files: Some(vec!["V1".into()]),
    };
    for (status, body, problem, identity) in [
        (
            200,
            r#"{"id":"1001","handle":"ada"}"#,
            None,
            IdentityEvidence::Verified(RemoteIdentity {
                id: "1001".into(),
                username: "ada".into(),
            }),
        ),
        (200, "{}", None, IdentityEvidence::Unnamed),
        (
            403,
            r#"{"status":403,"err":"Invalid token"}"#,
            Some(ProviderProblem::Refused),
            IdentityEvidence::Denied,
        ),
        (
            429,
            "{}",
            Some(ProviderProblem::Throttled),
            IdentityEvidence::None,
        ),
    ] {
        let server = TestServer::start(move |_| Reply::json(status, body));
        let result = check(&config, |config| FigmaApi::at(&server.url, config)).await;
        assert_eq!(result.outcome.problem(), problem);
        assert_eq!(
            matches!(result.outcome, CheckOutcome::Available),
            problem.is_none()
        );
        assert_eq!(result.identity, identity);
        assert_eq!(server.requests()[0].path(), "/v1/me");
    }
}
