use reqwest::{StatusCode, Url, header::HeaderName};
use serde_json::{Value, json};

use super::{
    PostJsonError, RemoteClient, Retry, SetupError, follows, identity_check, identity_from,
    test_server::{Reply, TestServer, closed_port, tls_client_builder},
};
use crate::{
    model::{IdentityEvidence, ProviderProblem, ProviderStatus, RemoteIdentity},
    provider::{
        http_client_builder,
        remote_error::{ApiError, OAuthRefusal},
    },
};

const TOKEN: &str = "example-token";

fn client(base: &str) -> RemoteClient {
    try_client(base).expect("a client")
}

fn try_client(base: &str) -> Result<RemoteClient, SetupError> {
    RemoteClient::new(
        base,
        HeaderName::from_static("private-token"),
        TOKEN,
        TOKEN,
        http_client_builder(),
    )
}

#[test]
fn endpoints_keep_the_context_path_and_drop_trailing_slashes() {
    for (base, expected) in [
        ("https://example.org", "https://example.org/api/v4/user"),
        ("https://example.org/", "https://example.org/api/v4/user"),
        ("https://example.org///", "https://example.org/api/v4/user"),
        (
            "https://example.org/jira",
            "https://example.org/jira/api/v4/user",
        ),
        (
            "https://example.org/jira//",
            "https://example.org/jira/api/v4/user",
        ),
        (
            " https://example.org/jira?x=1#top ",
            "https://example.org/jira/api/v4/user",
        ),
    ] {
        assert_eq!(
            client(base).endpoint(&["api", "v4", "user"]).as_str(),
            expected,
            "{base}"
        );
    }
}

#[test]
fn each_value_is_one_path_segment() {
    let api = client("https://example.org/gitlab");
    for (user, segment) in [
        ("jane/doe", "jane%2Fdoe"),
        ("a/../../admin", "a%2F..%2F..%2Fadmin"),
        ("..x", "..x"),
        ("who?state=all", "who%3Fstate=all"),
        ("name#frag", "name%23frag"),
        ("first last", "first%20last"),
        ("50%", "50%25"),
    ] {
        let url = api.endpoint(&["users", user, "events"]);
        assert_eq!(
            url.path(),
            format!("/gitlab/users/{segment}/events"),
            "{user}"
        );
        assert_eq!(url.host_str(), Some("example.org"));
        assert_eq!(url.query(), None);
        assert_eq!(url.fragment(), None);
    }
}

#[test]
fn query_values_survive_encoding() {
    let jql = r##"summary ~ "a & b" AND text ~ "#1" OR key = "X+1""##;
    let mut url = client("https://example.org").endpoint(&["search"]);
    url.query_pairs_mut().append_pair("jql", jql);
    let pairs: Vec<(String, String)> = url.query_pairs().into_owned().collect();
    assert_eq!(pairs, vec![("jql".to_string(), jql.to_string())]);
    assert_eq!(url.fragment(), None);
}

#[test]
fn urls_holding_credentials_are_refused() {
    for base in [
        "https://user:pass@example.org",
        "https://user@example.org/jira",
        "https://:pass@example.org",
    ] {
        assert_eq!(
            try_client(base).err(),
            Some(SetupError::Credentials),
            "{base}"
        );
    }
    assert_eq!(
        try_client("mailto:jane@example.org").err(),
        Some(SetupError::NotAnAddress)
    );
    let refused = SetupError::Credentials.check_result("Jira");
    assert_eq!(refused.outcome.status(), ProviderStatus::Unavailable);
    assert_eq!(
        refused.outcome.problem(),
        Some(ProviderProblem::NotConfigured)
    );
    assert_eq!(
        refused.outcome.reason(),
        Some(
            "Remove the user name and password from the address; the token goes in the token field."
        )
    );
}

#[test]
fn redirects_are_followed_only_to_the_same_origin() {
    let base = Url::parse("https://example.org:8443/jira").unwrap();
    let to = |next: &str| follows(&base, &Url::parse(next).unwrap());
    assert!(to("https://example.org:8443/jira/login"));
    assert!(to("https://EXAMPLE.org:8443/other"));
    assert!(!to("https://example.net:8443/jira"), "another host");
    assert!(!to("https://example.org:9443/jira"), "another port");
    assert!(!to("http://example.org:8443/jira"), "https to http");
    assert!(
        !to("https://example.org/jira"),
        "the default port is another port"
    );

    let default_port = Url::parse("https://example.org").unwrap();
    assert!(follows(
        &default_port,
        &Url::parse("https://example.org:443/x").unwrap()
    ));
}

#[tokio::test]
async fn a_same_origin_redirect_is_followed_with_the_token() {
    let server = TestServer::start(|request| match request.path() {
        "/old" => Reply::redirect("/new"),
        _ => Reply::json(200, r#"{"ok":true}"#),
    });
    let api = client(&server.url);

    let answer: Value = api.get_json(api.endpoint(&["old"])).await.unwrap();

    assert_eq!(answer["ok"], true);
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[1].path(), "/new");
    assert_eq!(requests[1].header("private-token"), Some(TOKEN));
}

#[tokio::test]
async fn a_redirect_to_http_on_the_same_host_and_port_is_not_followed() {
    let server = TestServer::start_tls(|request| {
        if request.path() == "/original" {
            let mut target = Url::parse("http://127.0.0.1/cleartext").unwrap();
            let (_, port) = request.header("host").unwrap().rsplit_once(':').unwrap();
            target.set_port(Some(port.parse().unwrap())).unwrap();
            Reply::redirect(target.as_str())
        } else {
            Reply::json(200, "{}")
        }
    });
    let api = RemoteClient::new(
        &server.url,
        HeaderName::from_static("private-token"),
        TOKEN,
        TOKEN,
        tls_client_builder(),
    )
    .unwrap();
    let answer = api.get_json::<Value>(api.endpoint(&["original"])).await;

    let requests = server.requests();
    let [request] = requests.as_slice() else {
        panic!("an HTTPS credential request crossed schemes");
    };
    assert_eq!(request.path(), "/original");
    assert_eq!(request.header("private-token"), Some(TOKEN));
    assert!(matches!(
        answer,
        Err(ApiError::Status {
            status: StatusCode::FOUND,
            ..
        })
    ));
}

#[tokio::test]
async fn a_redirect_to_another_port_or_host_is_not_followed() {
    let elsewhere = TestServer::start(|_| Reply::json(200, "{}"));
    let other_port = format!("{}/steal", elsewhere.url);
    let server = TestServer::start(move |request| match request.path() {
        "/port" => Reply::redirect(other_port.clone()),
        "/temporary" => Reply::redirect_with(307, other_port.clone()),
        "/permanent" => Reply::redirect_with(308, other_port.clone()),
        // The same port under another name: only the host differs.
        "/host" => Reply::redirect(format!(
            "http://localhost:{}/steal",
            request.header("host").unwrap().rsplit(':').next().unwrap()
        )),
        _ => Reply::json(200, "{}"),
    });
    let api = client(&server.url);

    for (path, status) in [
        ("port", StatusCode::FOUND),
        ("temporary", StatusCode::TEMPORARY_REDIRECT),
        ("permanent", StatusCode::PERMANENT_REDIRECT),
        ("host", StatusCode::FOUND),
    ] {
        let error = api
            .get_json::<Value>(api.endpoint(&[path]))
            .await
            .unwrap_err();
        assert!(
            matches!(&error, ApiError::Status { status: got, .. } if *got == status),
            "{path}: {error:?}"
        );
        assert_eq!(
            error.message("GitLab"),
            "GitLab redirected to another address; set the URL it redirects to"
        );
    }

    assert!(elsewhere.requests().is_empty(), "the token stayed home");
    let paths: Vec<_> = server
        .requests()
        .iter()
        .map(|request| request.path().to_string())
        .collect();
    assert_eq!(paths, ["/port", "/temporary", "/permanent", "/host"]);
}

#[tokio::test]
async fn a_json_post_retries_with_the_same_body_and_credentials() {
    let calls = std::sync::atomic::AtomicUsize::new(0);
    let server = TestServer::start(move |_| {
        if calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
            Reply::json(429, "{}").with_header("Retry-After", "0")
        } else {
            Reply::json(200, r#"{"data":{"user":{"login":"jdoe"}}}"#)
        }
    });
    let api = client(&server.url);
    let body = json!({
        "query": "query Find($login: String!) { user(login: $login) { login } }",
        "variables": {"login": "jdoe"}
    });

    let data: Value = api
        .post_graphql(api.endpoint(&["graphql"]), &body, Retry::OnceShort)
        .await
        .unwrap();

    assert_eq!(data["user"]["login"], "jdoe");
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    for request in requests {
        assert_eq!(request.method, "POST");
        assert_eq!(request.path(), "/graphql");
        assert_eq!(request.header("private-token"), Some(TOKEN));
        assert_eq!(request.header("content-type"), Some("application/json"));
        assert_eq!(
            request.header("user-agent"),
            Some(concat!("activity-log/", env!("CARGO_PKG_VERSION")))
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&request.body).unwrap(),
            body
        );
    }
}

#[tokio::test]
async fn a_post_does_not_send_credentials_or_body_to_another_origin() {
    let elsewhere = TestServer::start(|_| Reply::json(200, "{}"));
    let destination = format!("{}/graphql", elsewhere.url);
    let redirect = destination.clone();
    let server = TestServer::start(move |_| Reply::redirect_with(307, redirect.clone()));
    let api = client(&server.url);
    let body = json!({"variables":{"repo":"acme/widgets"}});

    let direct = api
        .post_json::<_, Value>(Url::parse(&destination).unwrap(), &body, Retry::Never)
        .await;
    let redirected = api
        .post_json::<_, Value>(api.endpoint(&["graphql"]), &body, Retry::Never)
        .await;

    assert!(matches!(
        direct,
        Err(PostJsonError::Remote(ApiError::Unreachable))
    ));
    assert!(matches!(
        redirected,
        Err(PostJsonError::Remote(ApiError::Status {
            status: StatusCode::TEMPORARY_REDIRECT,
            ..
        }))
    ));
    assert_eq!(server.requests().len(), 1);
    assert!(elsewhere.requests().is_empty());
}

#[tokio::test]
async fn a_json_post_reports_an_encoding_error_without_sending_a_request() {
    let server = TestServer::start(|_| Reply::json(200, "{}"));
    let api = client(&server.url);
    let cannot_encode = std::collections::BTreeMap::from([((1, 2), "value")]);

    let result = api
        .post_json::<_, Value>(api.endpoint(&["graphql"]), &cannot_encode, Retry::Never)
        .await;

    let Err(PostJsonError::Encode(error)) = result else {
        panic!("expected a payload encoding failure, got {result:?}");
    };
    assert!(!error.is_io());
    assert!(server.requests().is_empty());
}

#[tokio::test]
async fn a_url_of_another_origin_is_never_sent() {
    let elsewhere = TestServer::start(|_| Reply::json(200, "{}"));
    let api = client(&format!("http://127.0.0.1:{}", closed_port()));
    let url = Url::parse(&format!("{}/x", elsewhere.url)).unwrap();

    let error = api.get_json::<Value>(url).await.unwrap_err();

    assert_eq!(error, ApiError::Unreachable);
    assert!(elsewhere.requests().is_empty());
}

fn busy_once(status: u16, retry_after: String) -> TestServer {
    let calls = std::sync::atomic::AtomicUsize::new(0);
    TestServer::start(move |_| {
        if calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
            Reply::json(status, "{}").with_header("Retry-After", &retry_after)
        } else {
            Reply::json(200, r#"{"ok":true}"#)
        }
    })
}

#[tokio::test]
async fn a_short_retry_after_is_waited_for_and_retried_once() {
    // The date is made when its case runs: one made before the first case could be past by
    // then, and nothing would be waited for.
    let in_two_seconds = || {
        (chrono::Utc::now() + chrono::TimeDelta::seconds(2))
            .format("%a, %d %b %Y %H:%M:%S GMT")
            .to_string()
    };
    for retry_after in [|| "1".to_string(), in_two_seconds] {
        let retry_after = retry_after();
        let server = busy_once(503, retry_after.clone());
        let api = client(&server.url);
        let started = std::time::Instant::now();

        let answer: Value = api.get_json(api.endpoint(&["x"])).await.unwrap();

        assert_eq!(answer["ok"], true, "{retry_after}");
        assert_eq!(server.requests().len(), 2, "{retry_after}");
        assert!(
            started.elapsed() >= std::time::Duration::from_millis(900),
            "{retry_after}: waited {:?}",
            started.elapsed()
        );
    }
}

#[test]
fn only_busy_answers_with_a_short_wait_are_retried() {
    use reqwest::header::{HeaderMap, HeaderValue, RETRY_AFTER};

    for (status, retry_after, remaining, expected) in [
        (503, Some("10"), None, Some(10)),
        (429, Some("0"), None, Some(0)),
        (429, Some("11"), None, None),
        (503, None, None, None),
        (502, Some("1"), None, None),
        (403, Some("1"), None, Some(1)),
        (403, None, Some("0"), None),
    ] {
        let mut headers = HeaderMap::new();
        if let Some(value) = retry_after {
            headers.insert(RETRY_AFTER, HeaderValue::from_static(value));
        }
        if let Some(value) = remaining {
            headers.insert("X-RateLimit-Remaining", HeaderValue::from_static(value));
        }
        assert_eq!(
            super::retry_wait(
                StatusCode::from_u16(status).unwrap(),
                &headers,
                chrono::Utc::now()
            ),
            expected,
            "{status}, {headers:?}"
        );
    }
}

#[tokio::test]
async fn a_long_retry_after_is_returned_not_waited_for() {
    let server = TestServer::start(|_| Reply::json(429, "{}").with_header("Retry-After", "30"));
    let api = client(&server.url);

    let error = api
        .get_json::<Value>(api.endpoint(&["x"]))
        .await
        .unwrap_err();

    assert_eq!(
        error,
        ApiError::Status {
            status: StatusCode::TOO_MANY_REQUESTS,
            message: String::new(),
            retry_after: Some(30),
            quota: None,
            refusal: None,
        }
    );
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn a_second_busy_answer_is_not_retried_again() {
    let server = TestServer::start(|_| Reply::json(503, "{}").with_header("Retry-After", "1"));
    let api = client(&server.url);

    let error = api
        .get_json::<Value>(api.endpoint(&["x"]))
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        ApiError::Status {
            retry_after: Some(1),
            refusal: None,
            ..
        }
    ));
    assert_eq!(server.requests().len(), 2);
}

#[tokio::test]
async fn error_bodies_are_cleaned_and_html_never_shows() {
    let server = TestServer::start(|request| match request.path() {
        "/html" => Reply::html(500, "<html><h1>Internal error at /secret/path</h1></html>"),
        _ => Reply::json(
            400,
            format!(r#"{{"errorMessages":["bad token {TOKEN}\u001b[2J"]}}"#),
        ),
    });
    let api = client(&server.url);

    let html = api
        .get_json::<Value>(api.endpoint(&["html"]))
        .await
        .unwrap_err();
    assert_eq!(
        html.message("Jira"),
        "Jira answered 500 Internal Server Error"
    );

    let json = api
        .get_json::<Value>(api.endpoint(&["json"]))
        .await
        .unwrap_err();
    assert_eq!(
        json.message("Jira"),
        "Jira answered 400 Bad Request: bad token [redacted]\\u{1b}[2J"
    );
}

#[tokio::test]
async fn transport_failures_have_fixed_messages_without_the_url() {
    let api = client(&format!("http://127.0.0.1:{}/private?q=1", closed_port()));
    let error = api
        .get_json::<Value>(api.endpoint(&["x"]))
        .await
        .unwrap_err();
    assert_eq!(error, ApiError::Unreachable);
    assert_eq!(error.message("Jira"), "Could not reach Jira");

    let server = TestServer::start(|_| Reply::html(200, "<html>login</html>"));
    let api = client(&server.url);
    let error = api
        .get_json::<Value>(api.endpoint(&["x"]))
        .await
        .unwrap_err();
    assert_eq!(error, ApiError::Decode);
}

#[tokio::test]
async fn a_large_error_body_is_read_only_in_part() {
    let mut huge = String::from(r#"{"message":"#);
    huge.push_str(&"x".repeat(2 * 1024 * 1024));
    let server = TestServer::start(move |_| {
        Reply::json(500, huge.clone()).cut_off(4 * 1024 * 1024, std::time::Duration::from_secs(20))
    });
    let api = client(&server.url);
    let started = std::time::Instant::now();

    let error = api
        .get_json::<Value>(api.endpoint(&["x"]))
        .await
        .unwrap_err();

    assert_eq!(
        error.message("Jira"),
        "Jira answered 500 Internal Server Error"
    );
    assert!(
        started.elapsed() < std::time::Duration::from_secs(2),
        "stopped reading after the limit, took {:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn an_error_body_is_kept_to_its_limit() {
    let huge = "x".repeat(200 * 1024);
    let server = TestServer::start(move |_| Reply::json(500, huge.clone()));
    let response = http_client_builder()
        .build()
        .unwrap()
        .get(&server.url)
        .send()
        .await
        .unwrap();

    let (body, complete) = super::read_prefix(response, super::ERROR_BODY_LIMIT).await;

    assert!(!complete);
    assert_eq!(body.len(), 16 * 1024, "never more than the limit is kept");
}

#[tokio::test]
async fn an_error_body_that_stalls_keeps_its_status() {
    let server = TestServer::start(|_| {
        Reply::json(401, r#"{"message":"#).cut_off(1000, std::time::Duration::from_secs(20))
    });
    let api = client(&server.url);
    let started = std::time::Instant::now();

    let error = api
        .get_json::<Value>(api.endpoint(&["x"]))
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        ApiError::Status {
            status: StatusCode::UNAUTHORIZED,
            ..
        }
    ));
    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "the body is not waited for beyond its budget, took {:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn a_success_whose_body_is_cut_off_is_no_decode_error() {
    let server = TestServer::start(|_| {
        Reply::json(200, r#"{"id":4"#).cut_off(1000, std::time::Duration::ZERO)
    });
    let api = client(&server.url);

    let error = api
        .get_json::<Value>(api.endpoint(&["x"]))
        .await
        .unwrap_err();

    assert_eq!(error, ApiError::Unreachable);
}

#[test]
fn identity_answers_become_evidence() {
    use super::IdField::{Integer, Text};
    use serde_json::json;

    let named = |id: &str| {
        Ok(Some(RemoteIdentity {
            id: id.to_string(),
            username: "jdoe".to_string(),
        }))
    };
    assert_eq!(
        identity_from(
            Ok(json!({ "id": 42, "username": "jdoe" })),
            Integer("id"),
            "username"
        ),
        named("42")
    );
    assert_eq!(
        identity_from(
            Ok(json!({ "key": "JIRAUSER10100", "name": "jdoe" })),
            Text("key"),
            "name"
        ),
        named("JIRAUSER10100")
    );

    for unnamed in [
        json!({}),
        json!({ "id": null, "username": "jdoe" }),
        json!({ "id": "42" }),
        json!({ "id": 4.2 }),
        json!({ "id": -1 }),
        json!([1]),
    ] {
        assert_eq!(
            identity_from(Ok(unnamed.clone()), Integer("id"), "username"),
            Ok(None),
            "{unnamed}"
        );
    }
    for unnamed in [
        json!({ "key": "" }),
        json!({ "key": "  " }),
        json!({ "key": 42 }),
    ] {
        assert_eq!(
            identity_from(Ok(unnamed.clone()), Text("key"), "name"),
            Ok(None),
            "{unnamed}"
        );
    }
    assert_eq!(
        identity_from(
            Ok(json!({ "id": 7, "username": 3 })),
            Integer("id"),
            "username"
        ),
        Ok(Some(RemoteIdentity {
            id: "7".to_string(),
            username: String::new(),
        }))
    );
    for failed in [ApiError::Decode, ApiError::Timeout, ApiError::Unreachable] {
        assert_eq!(
            identity_from(Err(failed.clone()), Integer("id"), "username"),
            Err(failed)
        );
    }
}

#[test]
fn checks_map_identity_answers() {
    let identity = RemoteIdentity {
        id: "42".to_string(),
        username: "jdoe".to_string(),
    };
    let verified = identity_check("GitLab", Ok(Some(identity.clone())));
    assert_eq!(verified.outcome.status(), ProviderStatus::Available);
    assert_eq!(verified.identity, IdentityEvidence::Verified(identity));

    let unnamed = identity_check("GitLab", Ok(None));
    assert_eq!(unnamed.outcome.status(), ProviderStatus::Available);
    assert_eq!(unnamed.identity, IdentityEvidence::Unnamed);

    let status = |status, message: &str| {
        Err(ApiError::Status {
            status,
            message: message.to_string(),
            retry_after: None,
            quota: None,
            refusal: None,
        })
    };
    let denied = identity_check(
        "GitLab",
        status(StatusCode::UNAUTHORIZED, "401 Unauthorized"),
    );
    assert_eq!(denied.identity, IdentityEvidence::Denied);
    assert_eq!(
        denied.outcome.reason(),
        Some("GitLab credentials invalid"),
        "a message that only repeats the status is left out"
    );
    let expired = identity_check("Jira", status(StatusCode::UNAUTHORIZED, "Token expired"));
    assert_eq!(expired.identity, IdentityEvidence::Denied);
    assert_eq!(
        expired.outcome.reason(),
        Some("Jira credentials invalid: Token expired")
    );
    let bare = identity_check("Jira", status(StatusCode::FORBIDDEN, "Forbidden"));
    assert_eq!(bare.outcome.reason(), Some("Jira token lacks access"));

    let forbidden = identity_check(
        "GitLab",
        status(StatusCode::FORBIDDEN, "insufficient_scope"),
    );
    assert_eq!(forbidden.identity, IdentityEvidence::Denied);
    assert_eq!(
        forbidden.outcome.reason(),
        Some("GitLab token lacks access: insufficient_scope")
    );

    let failing = identity_check("GitLab", status(StatusCode::BAD_GATEWAY, ""));
    assert_eq!(failing.identity, IdentityEvidence::None);
    assert_eq!(failing.outcome.status(), ProviderStatus::Unavailable);
    assert_eq!(
        failing.outcome.reason(),
        Some("GitLab answered 502 Bad Gateway")
    );

    let unreadable = identity_check("GitLab", Err(ApiError::Decode));
    assert_eq!(unreadable.outcome.status(), ProviderStatus::Unavailable);
    assert_eq!(unreadable.outcome.problem(), Some(ProviderProblem::Failed));
    assert_eq!(
        unreadable.outcome.reason(),
        Some(
            "GitLab answered with a page that is not its API (a sign-in page?); check the URL and token"
        )
    );
    assert_eq!(unreadable.identity, IdentityEvidence::None);

    let cut_off = identity_check("GitLab", Err(ApiError::Unreachable));
    assert_eq!(cut_off.outcome.status(), ProviderStatus::Unavailable);
    assert_eq!(cut_off.identity, IdentityEvidence::None);

    let timeout = identity_check("GitLab", Err(ApiError::Timeout));
    assert_eq!(timeout.identity, IdentityEvidence::None);
    assert_eq!(
        timeout.outcome.reason(),
        Some("GitLab did not answer within 30 s")
    );
}

/// A refusal, a timeout and an unreachable server are told apart by their problem, so the UI
/// can say which; any other failure stays `failed`.
#[test]
fn a_check_names_its_problem_by_how_the_server_failed() {
    let status = |status| ApiError::Status {
        status,
        message: String::new(),
        retry_after: None,
        quota: None,
        refusal: None,
    };
    let problem = |answer: ApiError| identity_check("Jira", Err(answer)).outcome.problem();
    assert_eq!(
        problem(status(StatusCode::UNAUTHORIZED)),
        Some(ProviderProblem::Refused)
    );
    assert_eq!(
        problem(status(StatusCode::FORBIDDEN)),
        Some(ProviderProblem::Refused)
    );
    assert_eq!(problem(ApiError::Timeout), Some(ProviderProblem::TimedOut));
    assert_eq!(
        problem(ApiError::Unreachable),
        Some(ProviderProblem::Unreachable)
    );
    assert_eq!(
        problem(status(StatusCode::BAD_GATEWAY)),
        Some(ProviderProblem::Failed)
    );
    assert_eq!(problem(ApiError::Decode), Some(ProviderProblem::Failed));
}

#[tokio::test]
async fn a_busy_forbidden_answer_says_nothing_about_the_token_owner() {
    for (remaining, expected_problem, expected_identity) in [
        (
            Some("0"),
            ProviderProblem::Throttled,
            IdentityEvidence::None,
        ),
        (None, ProviderProblem::Refused, IdentityEvidence::Denied),
    ] {
        let server = TestServer::start(move |_| {
            let reply = Reply::json(403, "{}");
            match remaining {
                Some(remaining) => reply
                    .with_header("X-RateLimit-Remaining", remaining)
                    .with_header("WWW-Authenticate", r#"Bearer error="invalid_token""#),
                None => reply,
            }
        });
        let api = client(&server.url);
        let answer = api.get_json::<Value>(api.endpoint(&["user"])).await;
        let error = answer.unwrap_err();
        if remaining.is_some() {
            assert!(matches!(
                &error,
                ApiError::Status {
                    refusal: Some(OAuthRefusal::InvalidToken),
                    ..
                }
            ));
        }
        let checked = identity_check("GitHub", Err(error));

        assert_eq!(checked.outcome.status(), ProviderStatus::Unavailable);
        assert_eq!(checked.outcome.problem(), Some(expected_problem));
        assert_eq!(checked.identity, expected_identity);
        assert!(checked.outcome.recheckable());
        assert_eq!(server.requests().len(), 1);
    }
}

#[tokio::test]
async fn scope_classification_needs_typed_evidence_and_an_auth_status() {
    use crate::{
        provider::{remote_error::RemoteFailure, source_failure::ClassifiedProblem},
        runtime::CollectFailure,
    };

    for (status, oauth, busy, problem, identity) in [
        (
            401,
            Some("insufficient_scope"),
            false,
            Some(ProviderProblem::ScopeMissing),
            IdentityEvidence::None,
        ),
        (
            403,
            Some("insufficient_scope"),
            false,
            Some(ProviderProblem::ScopeMissing),
            IdentityEvidence::None,
        ),
        (
            403,
            Some("insufficient_scope"),
            true,
            Some(ProviderProblem::Throttled),
            IdentityEvidence::None,
        ),
        (
            403,
            Some("invalid_token"),
            false,
            Some(ProviderProblem::Refused),
            IdentityEvidence::Denied,
        ),
        (
            403,
            None,
            false,
            Some(ProviderProblem::Refused),
            IdentityEvidence::Denied,
        ),
        (
            500,
            Some("insufficient_scope"),
            false,
            None,
            IdentityEvidence::None,
        ),
    ] {
        let server = TestServer::start(move |_| {
            let mut reply = Reply::json(status, r#"{"message":"insufficient_scope"}"#);
            if let Some(oauth) = oauth {
                reply =
                    reply.with_header("WWW-Authenticate", &format!(r#"Bearer error="{oauth}""#));
            }
            if busy {
                reply = reply.with_header("X-RateLimit-Remaining", "0");
            }
            reply
        });
        let api = client(&server.url);
        let error = api
            .get_json::<Value>(api.endpoint(&["user"]))
            .await
            .unwrap_err();
        let check = identity_check("GitLab", Err(error.clone()));
        let failure = CollectFailure::of(
            &RemoteFailure {
                service: "GitLab".into(),
                error,
            }
            .into(),
        );

        assert_eq!(
            check.outcome.classification(),
            Some(ClassifiedProblem::plain(
                problem.unwrap_or(ProviderProblem::Failed)
            ))
        );
        assert_eq!(check.identity, identity);
        assert_eq!(
            failure.classification(),
            ClassifiedProblem::plain(problem.unwrap_or(ProviderProblem::CollectionFailed))
        );
        if problem == Some(ProviderProblem::ScopeMissing) {
            assert_eq!(
                check.outcome.reason(),
                Some("GitLab token is missing a required permission")
            );
        }
        assert_eq!(server.requests().len(), 1);
    }
}

/// A caller that acts on a busy answer itself gets it at once: one request. A wait comes only
/// before a second request, so the count proves there was none; a clock bound would fail under
/// load.
#[tokio::test]
async fn a_busy_answer_is_not_retried_for_a_caller_that_never_retries() {
    for status in [503, 429, 403] {
        let server =
            TestServer::start(move |_| Reply::json(status, "{}").with_header("Retry-After", "1"));
        let api = client(&server.url);

        let error = api
            .get_json_with_headers::<Value>(api.endpoint(&["x"]), Retry::Never)
            .await
            .unwrap_err();

        assert!(
            matches!(
                error,
                ApiError::Status {
                    retry_after: Some(1),
                    ..
                }
            ),
            "{error:?}"
        );
        assert_eq!(server.requests().len(), 1, "{status}");

        let server = busy_once(status, "0".to_string());
        let api = client(&server.url);
        let answer = api
            .get_json_with_headers::<Value>(api.endpoint(&["x"]), Retry::OnceShort)
            .await
            .unwrap();
        assert_eq!(answer.0["ok"], true);
        assert_eq!(server.requests().len(), 2);
    }
}

/// A refusal keeps the OAuth error code its answer named, from the challenge or the body.
#[tokio::test]
async fn a_refusal_keeps_its_oauth_code() {
    let server = TestServer::start(|request| match request.path() {
        "/body" => Reply::json(
            403,
            r#"{"error":"insufficient_scope","error_description":"x"}"#,
        ),
        "/challenge" => Reply::json(401, r#"{"message":"401 Unauthorized"}"#).with_header(
            "WWW-Authenticate",
            r#"Bearer realm="GitLab", error="invalid_token""#,
        ),
        _ => Reply::json(403, r#"{"message":"403 Forbidden"}"#),
    });
    let api = client(&server.url);
    let refusal = |path: &'static str| {
        let api = &api;
        async move {
            match api.get_json::<Value>(api.endpoint(&[path])).await {
                Err(ApiError::Status { refusal, .. }) => refusal,
                other => panic!("{other:?}"),
            }
        }
    };

    assert_eq!(refusal("body").await, Some(OAuthRefusal::InsufficientScope));
    assert_eq!(refusal("challenge").await, Some(OAuthRefusal::InvalidToken));
    assert_eq!(refusal("plain").await, None);
}
