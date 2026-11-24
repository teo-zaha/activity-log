use std::sync::Arc;

use crate::{
    model::{
        ConnectionId, IdentityEvidence, ProviderProblem, ProviderStatus, RemoteIdentity, Source,
    },
    provider::{
        connection::{Connection, ConnectionLocation, ConnectionValues},
        jira::{JiraConfig, JiraProvider},
        provider_runtime::ProviderRuntime,
        remote_api::test_server::{Reply, TestServer},
    },
};

use super::JiraApi;
use crate::provider::{
    provider_adapter::ProviderAdapter, provider_config::ProviderConfigResolve,
    provider_context::ProviderContext, provider_runtime::with_startup_timeout,
};

const TOKEN: &str = "jira-example-token";

pub(crate) fn jira_connection(url: &str) -> Connection {
    Connection::new(
        ConnectionId::new(Source::Jira, "work"),
        "Work Jira",
        ConnectionLocation::Local,
        ConnectionValues::new(
            [
                ("JIRA_URL".to_string(), Some(url.to_string())),
                ("JIRA_TOKEN".to_string(), Some(TOKEN.to_string())),
            ]
            .into(),
        ),
    )
}

async fn check(reply: impl Fn() -> Reply + Send + 'static) -> (ProviderRuntime, TestServer) {
    let server = TestServer::start(move |_| reply());
    let runtime = ProviderRuntime::inspect(
        Arc::new(JiraProvider),
        &jira_connection(&server.url),
        &crate::secrets::Secrets::none(),
    )
    .await;
    (runtime, server)
}

#[test]
fn endpoints_sit_below_the_context_path() {
    let config = JiraConfig {
        url: "https://example.org/jira//".to_string(),
        token: TOKEN.to_string(),
        insecure: false,
    };
    let api = JiraApi::new(&config).unwrap();
    assert_eq!(
        api.endpoint(&["issue", "ABC-1/../../myself"]).as_str(),
        "https://example.org/jira/rest/api/2/issue/ABC-1%2F..%2F..%2Fmyself"
    );
}

#[tokio::test]
async fn the_check_names_the_token_owner() {
    let (runtime, server) = check(|| {
        Reply::json(
            200,
            r#"{"key":"JIRAUSER10100","name":"jdoe","displayName":"J"}"#,
        )
    })
    .await;

    assert!(runtime.usable(), "{:?}", runtime.info().reason);
    assert_eq!(
        runtime.identity(),
        &IdentityEvidence::Verified(RemoteIdentity {
            id: "JIRAUSER10100".to_string(),
            username: "jdoe".to_string(),
        })
    );
    let requests = server.requests();
    assert_eq!(requests[0].path(), "/rest/api/2/myself");
    assert_eq!(
        requests[0].header("authorization"),
        Some(format!("Bearer {TOKEN}").as_str())
    );
}

#[tokio::test]
async fn a_named_header_verifies_the_account() {
    for name in ["jdoe", "Anonymous"] {
        let (runtime, _server) = check(move || {
            Reply::json(200, format!(r#"{{"key":"JIRAUSER10100","name":"{name}"}}"#))
                .with_header("X-AUSERNAME", name)
        })
        .await;

        assert_eq!(runtime.info().status, ProviderStatus::Available, "{name}");
        assert_eq!(
            runtime.identity(),
            &IdentityEvidence::Verified(RemoteIdentity {
                id: "JIRAUSER10100".into(),
                username: name.into(),
            })
        );
    }
}

#[tokio::test]
async fn an_anonymous_reply_refuses_the_token_even_with_a_key() {
    let (runtime, _server) = check(|| {
        Reply::json(200, r#"{"key":"JIRAUSER10100","name":"jdoe"}"#)
            .with_header("X-AUSERNAME", "anonymous")
    })
    .await;

    assert_eq!(runtime.info().status, ProviderStatus::Unavailable);
    assert_eq!(runtime.info().problem, Some(ProviderProblem::Refused));
    assert_eq!(runtime.identity(), &IdentityEvidence::None);
}

#[tokio::test]
async fn a_success_without_a_usable_key_refuses_the_connection() {
    for body in [
        r#"{"name":"jdoe"}"#,
        r#"{"key":10100,"name":"jdoe"}"#,
        r#"{"key":" ","name":"jdoe"}"#,
    ] {
        let (runtime, _server) = check(move || Reply::json(200, body)).await;

        assert_eq!(runtime.info().status, ProviderStatus::Unavailable, "{body}");
        assert_eq!(runtime.info().problem, Some(ProviderProblem::Refused));
        assert_eq!(runtime.identity(), &IdentityEvidence::None);
    }
}

#[tokio::test]
async fn refused_tokens_are_denied() {
    let (unauthorized, _server) = check(|| Reply::json(401, "{}")).await;
    assert!(!unauthorized.usable());
    assert_eq!(unauthorized.identity(), &IdentityEvidence::Denied);
    assert_eq!(
        unauthorized.info().reason.as_deref(),
        Some("Jira credentials invalid")
    );

    let (forbidden, _server) = check(|| Reply::json(403, "{}")).await;
    assert_eq!(forbidden.identity(), &IdentityEvidence::Denied);
    assert_eq!(
        forbidden.info().reason.as_deref(),
        Some("Jira token lacks access")
    );
}

#[tokio::test]
async fn a_throttled_jira_check_is_unavailable_and_recheckable() {
    let (runtime, _server) = check(|| Reply::json(429, "{}")).await;

    assert_eq!(runtime.info().status, ProviderStatus::Unavailable);
    assert_eq!(runtime.info().problem, Some(ProviderProblem::Throttled));
    assert_eq!(runtime.identity(), &IdentityEvidence::None);
    assert!(runtime.info().recheckable);
}

#[tokio::test]
async fn a_server_error_says_nothing_about_the_owner_and_keeps_its_message() {
    let (runtime, _server) = check(|| {
        Reply::json(
            503,
            format!(r#"{{"errorMessages":["Maintenance until 10:00 for {TOKEN}"]}}"#),
        )
    })
    .await;

    assert!(!runtime.usable());
    assert_eq!(runtime.identity(), &IdentityEvidence::None);
    assert_eq!(
        runtime.info().reason.as_deref(),
        Some("Jira answered 503 Service Unavailable: Maintenance until 10:00 for [redacted]")
    );
}

#[tokio::test]
async fn a_url_with_credentials_is_not_configured_and_never_asked() {
    let server = TestServer::start(|_| Reply::json(200, r#"{"key":"x"}"#));
    let url = server.url.replace("http://", "http://jane:secret@");

    let runtime = ProviderRuntime::inspect(
        Arc::new(JiraProvider),
        &jira_connection(&url),
        &crate::secrets::Secrets::none(),
    )
    .await;

    assert!(!runtime.usable());
    assert_eq!(runtime.info().problem, Some(ProviderProblem::NotConfigured));
    assert_eq!(
        runtime.info().reason.as_deref(),
        Some(
            "Remove the user name and password from the address; the token goes in the token field."
        )
    );
    assert_eq!(runtime.identity(), &IdentityEvidence::None);
    assert!(server.requests().is_empty());
}

#[tokio::test]
async fn a_denial_whose_body_stalls_or_breaks_is_still_denied() {
    for linger in [
        std::time::Duration::ZERO,
        std::time::Duration::from_secs(30),
    ] {
        let started = std::time::Instant::now();
        let (runtime, _server) =
            check(move || Reply::json(401, r#"{"errorMessages":["#).cut_off(5000, linger)).await;

        assert_eq!(runtime.identity(), &IdentityEvidence::Denied, "{linger:?}");
        assert_eq!(
            runtime.info().reason.as_deref(),
            Some("Jira credentials invalid")
        );
        assert!(
            started.elapsed() < std::time::Duration::from_secs(10),
            "well inside the startup deadline, took {:?}",
            started.elapsed()
        );
    }
}

#[tokio::test]
async fn a_success_whose_body_is_cut_off_names_no_one_and_is_not_usable() {
    let (runtime, _server) =
        check(|| Reply::json(200, r#"{"key":"JIRAUSER1"#).cut_off(1000, std::time::Duration::ZERO))
            .await;

    assert!(!runtime.usable());
    assert_eq!(runtime.identity(), &IdentityEvidence::None);
    assert_eq!(
        runtime.info().reason.as_deref(),
        Some("Could not reach Jira")
    );
}

#[tokio::test]
async fn a_sign_in_page_instead_of_the_api_leaves_the_connection_unavailable() {
    let (runtime, _server) = check(|| Reply::html(200, "<html>Log in</html>")).await;
    assert!(!runtime.usable());
    assert_eq!(runtime.info().problem, Some(ProviderProblem::Failed));
    assert_eq!(
        runtime.info().reason.as_deref(),
        Some(
            "Jira answered with a page that is not its API (a sign-in page?); check the URL and token"
        )
    );
    assert_eq!(runtime.identity(), &IdentityEvidence::None);
}

#[tokio::test]
async fn a_denial_whose_headers_come_near_the_deadline_is_still_denied() {
    // The check deadline is 2 s here (15 s in the app); the refusal arrives 1.3 s after the
    // check started, whatever setting up the client took, and its body never ends.
    let started = std::time::Instant::now();
    let headers_at = started + std::time::Duration::from_millis(1300);
    let server = TestServer::start(move |_| {
        Reply::json(401, r#"{"errorMessages":["#)
            .after(headers_at.saturating_duration_since(std::time::Instant::now()))
            .cut_off(5000, std::time::Duration::from_secs(30))
    });
    let mut context =
        ProviderContext::for_connection(&jira_connection(&server.url), JiraConfig::requirements());
    context.with_typed::<JiraConfig>().unwrap();

    let result = with_startup_timeout(
        JiraProvider.startup_check(context),
        std::time::Duration::from_secs(2),
    )
    .await;

    assert_eq!(result.identity, IdentityEvidence::Denied, "{result:?}");
    assert_eq!(result.outcome.problem(), Some(ProviderProblem::Refused));
    assert_eq!(result.outcome.reason(), Some("Jira credentials invalid"));
}

#[tokio::test]
async fn a_busy_check_keeps_its_answer_when_the_retry_cannot_fit() {
    let server = TestServer::start(|_| Reply::json(429, "{}").with_header("Retry-After", "3"));
    let mut context =
        ProviderContext::for_connection(&jira_connection(&server.url), JiraConfig::requirements());
    context.with_typed::<JiraConfig>().unwrap();

    let result = with_startup_timeout(
        JiraProvider.startup_check(context),
        std::time::Duration::from_secs(2),
    )
    .await;

    assert_eq!(result.outcome.problem(), Some(ProviderProblem::Throttled));
    assert_eq!(result.identity, IdentityEvidence::None);
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn a_busy_check_retries_and_can_verify_the_identity() {
    let (result, server) = check_busy_retry(
        Reply::json(200, r#"{"key":"JIRAUSER1","name":"jdoe"}"#),
        std::time::Duration::from_secs(5),
    )
    .await;

    assert_eq!(server.requests().len(), 2);
    assert_eq!(result.outcome.status(), ProviderStatus::Available);
    assert_eq!(
        result.identity,
        IdentityEvidence::Verified(RemoteIdentity {
            id: "JIRAUSER1".into(),
            username: "jdoe".into(),
        })
    );
}

#[tokio::test]
async fn a_busy_check_keeps_its_answer_when_the_retried_body_stalls() {
    let (result, server) = check_busy_retry(
        Reply::json(200, r#"{"key":"#).cut_off(1000, std::time::Duration::from_secs(5)),
        std::time::Duration::from_secs(2),
    )
    .await;

    assert_eq!(server.requests().len(), 2);
    assert_eq!(result.outcome.problem(), Some(ProviderProblem::Throttled));
    assert_eq!(result.identity, IdentityEvidence::None);
}

async fn check_busy_retry(
    reply: Reply,
    timeout: std::time::Duration,
) -> (
    crate::provider::provider_check_result::ProviderCheckResult,
    TestServer,
) {
    let reply = std::cell::RefCell::new(Some(reply));
    let tried = std::cell::Cell::new(false);
    let server = TestServer::start(move |_| {
        if tried.replace(true) {
            reply.borrow_mut().take().unwrap()
        } else {
            Reply::json(429, "{}").with_header("Retry-After", "0")
        }
    });
    let mut context =
        ProviderContext::for_connection(&jira_connection(&server.url), JiraConfig::requirements());
    context.with_typed::<JiraConfig>().unwrap();

    let result = with_startup_timeout(JiraProvider.startup_check(context), timeout).await;
    (result, server)
}
