use reqwest::{StatusCode, Url, header::HeaderName};
use serde_json::Value;

use super::{
    RemoteClient, SetupError, follows, identity_check, identity_from,
    test_server::{Reply, TestServer, closed_port},
};
use crate::{
    model::{IdentityEvidence, ProviderProblem, ProviderStatus, RemoteIdentity},
    provider::{http_client_builder, remote_error::ApiError},
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
    assert_eq!(refused.status, ProviderStatus::Unavailable);
    assert_eq!(refused.problem, Some(ProviderProblem::NotConfigured));
    assert_eq!(
        refused.reason.as_deref(),
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
async fn a_url_of_another_origin_is_never_sent() {
    let elsewhere = TestServer::start(|_| Reply::json(200, "{}"));
    let api = client(&format!("http://127.0.0.1:{}", closed_port()));
    let url = Url::parse(&format!("{}/x", elsewhere.url)).unwrap();

    let error = api.get_json::<Value>(url).await.unwrap_err();

    assert_eq!(error, ApiError::Unreachable);
    assert!(elsewhere.requests().is_empty());
}

fn busy_once(retry_after: String) -> TestServer {
    let calls = std::sync::atomic::AtomicUsize::new(0);
    TestServer::start(move |_| {
        if calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
            Reply::json(503, "{}").with_header("Retry-After", &retry_after)
        } else {
            Reply::json(200, r#"{"ok":true}"#)
        }
    })
}

#[tokio::test]
async fn a_short_retry_after_is_waited_for_and_retried_once() {
    let in_two_seconds = (chrono::Utc::now() + chrono::TimeDelta::seconds(2))
        .format("%a, %d %b %Y %H:%M:%S GMT")
        .to_string();
    for retry_after in ["1".to_string(), in_two_seconds] {
        let server = busy_once(retry_after.clone());
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
fn retry_after_reads_seconds_and_http_dates() {
    use chrono::{TimeZone, Utc};
    use reqwest::header::{HeaderMap, HeaderValue, RETRY_AFTER};

    let now = Utc.with_ymd_and_hms(2026, 11, 19, 8, 0, 0).unwrap();
    let read = |value: &str| {
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, HeaderValue::from_str(value).unwrap());
        super::retry_after(&headers, now)
    };
    assert_eq!(read("7"), Some(7));
    assert_eq!(read(" 0 "), Some(0));
    assert_eq!(read("Thu, 19 Nov 2026 08:00:05 GMT"), Some(5));
    assert_eq!(read("Thursday, 19-Nov-26 08:00:30 GMT"), Some(30));
    assert_eq!(read("Thu Nov 19 08:00:10 2026"), Some(10));
    assert_eq!(
        read("Thu, 19 Nov 2026 07:59:00 GMT"),
        Some(0),
        "in the past"
    );
    assert_eq!(read("Thu, 19 Nov 2026 08:00:04.5 GMT"), None);
    assert_eq!(read("-3"), None);
    assert_eq!(read("soon"), None);
    assert_eq!(super::retry_after(&HeaderMap::new(), now), None);
}

#[test]
fn only_busy_answers_with_a_short_wait_are_retried() {
    use super::retry_wait;
    assert_eq!(
        retry_wait(StatusCode::SERVICE_UNAVAILABLE, Some(10)),
        Some(10)
    );
    assert_eq!(retry_wait(StatusCode::TOO_MANY_REQUESTS, Some(0)), Some(0));
    assert_eq!(retry_wait(StatusCode::TOO_MANY_REQUESTS, Some(11)), None);
    assert_eq!(retry_wait(StatusCode::SERVICE_UNAVAILABLE, None), None);
    assert_eq!(retry_wait(StatusCode::BAD_GATEWAY, Some(1)), None);
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
    assert_eq!(verified.status, ProviderStatus::Available);
    assert_eq!(verified.identity, IdentityEvidence::Verified(identity));

    let unnamed = identity_check("GitLab", Ok(None));
    assert_eq!(unnamed.status, ProviderStatus::Available);
    assert_eq!(unnamed.identity, IdentityEvidence::Unnamed);

    let status = |status, message: &str| {
        Err(ApiError::Status {
            status,
            message: message.to_string(),
            retry_after: None,
        })
    };
    let denied = identity_check(
        "GitLab",
        status(StatusCode::UNAUTHORIZED, "401 Unauthorized"),
    );
    assert_eq!(denied.identity, IdentityEvidence::Denied);
    assert_eq!(
        denied.reason.as_deref(),
        Some("GitLab credentials invalid"),
        "a message that only repeats the status is left out"
    );
    let expired = identity_check("Jira", status(StatusCode::UNAUTHORIZED, "Token expired"));
    assert_eq!(expired.identity, IdentityEvidence::Denied);
    assert_eq!(
        expired.reason.as_deref(),
        Some("Jira credentials invalid: Token expired")
    );
    let bare = identity_check("Jira", status(StatusCode::FORBIDDEN, "Forbidden"));
    assert_eq!(bare.reason.as_deref(), Some("Jira token lacks access"));

    let forbidden = identity_check(
        "GitLab",
        status(StatusCode::FORBIDDEN, "insufficient_scope"),
    );
    assert_eq!(forbidden.identity, IdentityEvidence::Denied);
    assert_eq!(
        forbidden.reason.as_deref(),
        Some("GitLab token lacks access: insufficient_scope")
    );

    let failing = identity_check("GitLab", status(StatusCode::BAD_GATEWAY, ""));
    assert_eq!(failing.identity, IdentityEvidence::None);
    assert_eq!(failing.status, ProviderStatus::Unavailable);
    assert_eq!(
        failing.reason.as_deref(),
        Some("GitLab answered 502 Bad Gateway")
    );

    let unreadable = identity_check("GitLab", Err(ApiError::Decode));
    assert_eq!(unreadable.status, ProviderStatus::Unavailable);
    assert_eq!(unreadable.problem, Some(ProviderProblem::Failed));
    assert_eq!(
        unreadable.reason.as_deref(),
        Some(
            "GitLab answered with a page that is not its API (a sign-in page?); check the URL and token"
        )
    );
    assert_eq!(unreadable.identity, IdentityEvidence::None);

    let cut_off = identity_check("GitLab", Err(ApiError::Unreachable));
    assert_eq!(cut_off.status, ProviderStatus::Unavailable);
    assert_eq!(cut_off.identity, IdentityEvidence::None);

    let timeout = identity_check("GitLab", Err(ApiError::Timeout));
    assert_eq!(timeout.identity, IdentityEvidence::None);
    assert_eq!(
        timeout.reason.as_deref(),
        Some("GitLab did not answer within 30 s")
    );
}
