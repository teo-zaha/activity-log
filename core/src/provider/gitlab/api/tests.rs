use std::sync::Arc;

use crate::{
    model::{ConnectionId, IdentityEvidence, RemoteIdentity, Source},
    provider::{
        connection::{Connection, ConnectionLocation, ConnectionValues},
        gitlab::GitLabProvider,
        provider_runtime::ProviderRuntime,
        remote_api::test_server::{Reply, TestServer},
    },
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
async fn a_success_without_an_id_is_unnamed() {
    let (runtime, _server) = check(|| Reply::json(200, r#"{"username":"jdoe"}"#)).await;
    assert!(runtime.usable());
    assert_eq!(runtime.identity(), &IdentityEvidence::Unnamed);
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

    let (forbidden, _server) = check(|| {
        Reply::json(
            403,
            r#"{"error":"insufficient_scope","error_description":"x"}"#,
        )
    })
    .await;
    assert_eq!(forbidden.identity(), &IdentityEvidence::Denied);
    assert_eq!(
        forbidden.info().reason.as_deref(),
        Some("GitLab token lacks access: insufficient_scope")
    );
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
async fn an_id_that_is_no_whole_number_names_no_one() {
    for body in [
        r#"{"id":"42","username":"jdoe"}"#,
        r#"{"id":4.2}"#,
        r#"{"id":-1}"#,
    ] {
        let (runtime, _server) = check(move || Reply::json(200, body)).await;
        assert!(runtime.usable(), "{body}");
        assert_eq!(runtime.identity(), &IdentityEvidence::Unnamed, "{body}");
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
