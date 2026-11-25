use std::sync::Arc;

use crate::{
    model::{
        CollectOptions, ConnectionId, IdentityEvidence, ProviderProblem, RemoteIdentity, Source,
    },
    provider::{
        connection::{Connection, ConnectionLocation, ConnectionValues},
        gitlab::GitLabProvider,
        provider_runtime::ProviderRuntime,
        remote_api::test_server::{Reply, TestServer},
    },
    runtime::report::Slot,
};

pub(crate) const TOKEN: &str = "glpat-example-token";

pub(crate) fn gitlab_connection(url: &str, user: &str) -> Connection {
    Connection::new(
        ConnectionId::new(Source::Gitlab, "work"),
        "Work GitLab",
        ConnectionLocation::Local,
        ConnectionValues::new(
            [
                ("GITLAB_URL".to_string(), Some(url.to_string())),
                ("GITLAB_TOKEN".to_string(), Some(TOKEN.to_string())),
                ("GITLAB_USER".to_string(), Some(user.to_string())),
            ]
            .into(),
        ),
    )
}

async fn check(reply: impl Fn() -> Reply + Send + 'static) -> (ProviderRuntime, TestServer) {
    let server = TestServer::start(move |_| reply());
    let runtime = ProviderRuntime::inspect(
        Arc::new(GitLabProvider),
        &gitlab_connection(&format!("{}/gitlab/", server.url), "someone-else"),
        &crate::secrets::Secrets::none(),
    )
    .await;
    (runtime, server)
}

#[tokio::test]
async fn the_check_names_the_token_owner_not_the_configured_user() {
    let (runtime, server) = check(|| Reply::json(200, r#"{"id":42,"username":"jdoe"}"#)).await;

    assert!(runtime.usable(), "{:?}", runtime.info().reason);
    assert_eq!(
        runtime.identity(),
        &IdentityEvidence::Verified(RemoteIdentity {
            id: "42".to_string(),
            username: "jdoe".to_string(),
        })
    );
    let requests = server.requests();
    assert_eq!(requests[0].path(), "/gitlab/api/v4/user");
    assert_eq!(requests[0].header("private-token"), Some(TOKEN));
}

#[tokio::test]
async fn a_nameless_gitlab_answer_never_starts_collection() {
    let (runtime, server) = check(|| Reply::json(200, r#"{"username":"jdoe"}"#)).await;
    let range = CollectOptions {
        start: "2026-11-01T00:00:00Z".parse().unwrap(),
        end: "2026-11-02T00:00:00Z".parse().unwrap(),
    };
    let report = crate::collect_runtimes(range, std::slice::from_ref(&runtime)).await;

    assert!(matches!(report.slots.as_slice(), [(_, Slot::Skipped)]));
    assert_eq!(runtime.info().problem, Some(ProviderProblem::Failed));
    assert_eq!(runtime.identity(), &IdentityEvidence::None);
    let requests = server.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].path(), "/gitlab/api/v4/user");
}

#[tokio::test]
async fn refused_tokens_are_denied() {
    let (unauthorized, _server) =
        check(|| Reply::json(401, r#"{"message":"401 Unauthorized"}"#)).await;
    assert!(!unauthorized.usable());
    assert_eq!(unauthorized.identity(), &IdentityEvidence::Denied);
    assert_eq!(
        unauthorized.info().reason.as_deref(),
        Some("GitLab credentials invalid")
    );

    let (forbidden, _server) = check(|| Reply::json(403, r#"{"message":"Forbidden"}"#)).await;
    assert!(!forbidden.usable());
    assert_eq!(forbidden.info().problem, Some(ProviderProblem::Refused));
    assert_eq!(forbidden.identity(), &IdentityEvidence::Denied);
}

#[tokio::test]
async fn missing_scope_does_not_deny_the_identity() {
    let (missing_scope, _server) = check(|| {
        Reply::json(
            403,
            r#"{"error":"insufficient_scope","error_description":"x"}"#,
        )
    })
    .await;
    assert!(!missing_scope.usable());
    assert_eq!(
        missing_scope.info().problem,
        Some(ProviderProblem::ScopeMissing)
    );
    assert_eq!(missing_scope.identity(), &IdentityEvidence::None);
}

#[tokio::test]
async fn a_server_error_says_nothing_about_the_owner() {
    let (runtime, _server) = check(|| Reply::html(502, "<html>bad gateway</html>")).await;
    assert!(!runtime.usable());
    assert_eq!(runtime.identity(), &IdentityEvidence::None);
    assert_eq!(
        runtime.info().reason.as_deref(),
        Some("GitLab answered 502 Bad Gateway")
    );
}

#[tokio::test]
async fn an_invalid_account_id_leaves_gitlab_unavailable() {
    for body in [
        r#"{"id":"42","username":"jdoe"}"#,
        r#"{"id":4.2}"#,
        r#"{"id":-1}"#,
    ] {
        let (runtime, _server) = check(move || Reply::json(200, body)).await;
        assert!(!runtime.usable(), "{body}");
        assert_eq!(
            runtime.info().problem,
            Some(ProviderProblem::Failed),
            "{body}"
        );
        assert_eq!(runtime.identity(), &IdentityEvidence::None, "{body}");
    }
}

#[tokio::test]
async fn an_explanation_of_a_denial_is_kept() {
    let (runtime, _server) = check(|| Reply::json(401, r#"{"message":"Token has expired"}"#)).await;
    assert_eq!(runtime.identity(), &IdentityEvidence::Denied);
    assert_eq!(
        runtime.info().reason.as_deref(),
        Some("GitLab credentials invalid: Token has expired")
    );
}

#[tokio::test]
async fn a_sign_in_page_instead_of_the_api_leaves_the_connection_unavailable() {
    let (runtime, _server) = check(|| Reply::html(200, "<html>Sign in</html>")).await;
    assert!(!runtime.usable());
    assert_eq!(
        runtime.info().reason.as_deref(),
        Some(
            "GitLab answered with a page that is not its API (a sign-in page?); check the URL and token"
        )
    );
    assert_eq!(runtime.identity(), &IdentityEvidence::None);
}
