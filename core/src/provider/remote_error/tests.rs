use reqwest::{
    StatusCode,
    header::{HeaderMap, HeaderValue, WWW_AUTHENTICATE},
};

use super::{ApiError, OAuthRefusal, remote_message};

const TOKEN: &str = "glpat-example-secret";

#[test]
fn keeps_only_the_explanation_fields() {
    let jira = r#"{"errorMessages":["Field 'x' does not exist"],"errors":{"jql":"Bad query"},"stack":"at Foo"}"#;
    assert_eq!(
        remote_message(jira, &[]),
        "Field 'x' does not exist; jql: Bad query"
    );

    let gitlab = r#"{"message":{"name":["has already been taken"]},"trace":"x"}"#;
    assert_eq!(remote_message(gitlab, &[]), "name: has already been taken");
    assert_eq!(
        remote_message(r#"{"error":"insufficient_scope"}"#, &[]),
        "insufficient_scope"
    );
    assert_eq!(
        remote_message(r#"{"message":"404 Not Found"}"#, &[]),
        "404 Not Found"
    );
}

#[test]
fn an_html_or_unexpected_body_never_appears() {
    assert_eq!(
        remote_message("<html><body>Proxy error at 10.0.0.1</body></html>", &[]),
        ""
    );
    assert_eq!(remote_message(r#"["a list"]"#, &[]), "");
    assert_eq!(remote_message(r#"{"detail":"elsewhere"}"#, &[]), "");
    assert_eq!(remote_message("", &[]), "");
}

#[test]
fn secrets_inside_the_allowed_fields_are_redacted() {
    let body = format!(r#"{{"errorMessages":["token {TOKEN} rejected"]}}"#);
    let message = remote_message(&body, &[TOKEN, ""]);
    assert_eq!(message, "token [redacted] rejected");
}

#[test]
fn urls_lose_their_credentials_query_and_fragment() {
    let body = r#"{"message":"see https://jane:pw@proxy.example.org:8080/login?token=abc&x=1#top, or (http://user@example.org/a?private_token=t). Done","error":"ftp://x:y@example.org/f?k=v"}"#;
    assert_eq!(
        remote_message(body, &[]),
        "see https://proxy.example.org:8080/login, or (http://example.org/a). Done; ftp://example.org/f"
    );
    assert_eq!(
        remote_message(r#"{"message":"bad https://[::1:/x?y=z and ://lone"}"#, &[]),
        "bad [redacted] and ://lone"
    );
}

#[test]
fn urls_with_sub_delimiters_are_reduced_whole() {
    let body = r#"{"message":"see https://example.org/people/O'Neill?token=abc&x=it's, or <https://us'er:p(a)ss@example.org/a(b)*!;,$@~%20/c?k='v'&t=(s)#f'g> and 'https://example.org/q?private_token=t'."}"#;
    let message = remote_message(body, &[]);
    assert_eq!(
        message,
        "see https://example.org/people/O'Neill, or <https://example.org/a(b)*!;,$@~%20/c> and 'https://example.org/q'."
    );
    for leaked in [
        "token", "abc", "it's", "us'er", "p(a)ss", "k=", "(s)", "f'g",
    ] {
        assert!(!message.contains(leaked), "{leaked} in {message}");
    }
}

#[test]
fn control_and_bidirectional_characters_are_escaped() {
    let body =
        "{\"message\":\"clear\\u001b[2J now \\u202Edesrever\\u2066x\\u2069 line\\r\\nnext\"}";
    let message = remote_message(body, &[]);
    assert_eq!(
        message,
        "clear\\u{1b}[2J now \\u{202e}desrever\\u{2066}x\\u{2069} line\\u{d}\\u{a}next"
    );
    assert!(!message.chars().any(char::is_control));
}

#[test]
fn long_messages_are_cut_to_300_characters() {
    let body = format!(r#"{{"message":"{}"}}"#, "é".repeat(1000));
    let message = remote_message(&body, &[]);
    assert_eq!(message.chars().count(), 300);
    assert!(message.ends_with('…'));

    let escapes = format!(r#"{{"message":"{}"}}"#, "\\u001b".repeat(200));
    assert_eq!(remote_message(&escapes, &[]).chars().count(), 300);
}

#[test]
fn messages_name_the_service_and_never_a_url() {
    let status = |status, message: &str| ApiError::Status {
        status,
        message: message.to_string(),
        retry_after: None,
        refusal: None,
    };
    assert_eq!(
        status(StatusCode::BAD_GATEWAY, "").message("Jira"),
        "Jira answered 502 Bad Gateway"
    );
    assert_eq!(
        status(StatusCode::NOT_FOUND, "404 User Not Found").message("GitLab"),
        "GitLab answered 404 Not Found: 404 User Not Found"
    );
    assert_eq!(
        status(StatusCode::FOUND, "").message("Jira"),
        "Jira redirected to another address; set the URL it redirects to"
    );
    assert_eq!(
        ApiError::Timeout.message("Jira"),
        "Jira did not answer within 30 s"
    );
    assert_eq!(
        ApiError::Unreachable.message("GitLab"),
        "Could not reach GitLab"
    );
    assert_eq!(
        ApiError::Decode.message("Jira"),
        "Jira sent an answer Activity Log could not read"
    );
}

fn challenges(values: &[&str]) -> HeaderMap {
    let mut headers = HeaderMap::new();
    for value in values {
        headers.append(WWW_AUTHENTICATE, HeaderValue::from_str(value).unwrap());
    }
    headers
}

/// The OAuth error code is read from the body's `error` field as the protocol code it is; a
/// code RFC 6750 does not define, or none, names no refusal.
#[test]
fn an_oauth_refusal_is_read_from_the_body() {
    let none = HeaderMap::new();
    for (body, refusal) in [
        (
            r#"{"error":"insufficient_scope","error_description":"The request requires higher privileges.","scope":"api read_api"}"#,
            Some(OAuthRefusal::InsufficientScope),
        ),
        (
            r#"{"error":"invalid_token"}"#,
            Some(OAuthRefusal::InvalidToken),
        ),
        (
            r#"{"error":"invalid_request"}"#,
            Some(OAuthRefusal::InvalidRequest),
        ),
        (r#"{"error":"Insufficient_Scope"}"#, None),
        (r#"{"error":"insufficient_scope "}"#, None),
        (r#"{"error":"server_error"}"#, None),
        (r#"{"error":{"code":"insufficient_scope"}}"#, None),
        (r#"{"message":"insufficient_scope"}"#, None),
        (r#"{"error_description":"insufficient_scope"}"#, None),
        ("<html>insufficient_scope</html>", None),
        ("", None),
    ] {
        assert_eq!(OAuthRefusal::of_answer(&none, body), refusal, "{body}");
    }
}

#[test]
fn an_oauth_refusal_is_read_from_the_bearer_challenge() {
    for (values, refusal) in [
        (
            &[
                r#"Bearer realm="GitLab", error="insufficient_scope", error_description="The request requires higher privileges than provided by the access token.""#,
            ][..],
            Some(OAuthRefusal::InsufficientScope),
        ),
        (
            &[r#"bearer error=invalid_token"#][..],
            Some(OAuthRefusal::InvalidToken),
        ),
        (
            &[r#"Bearer error_description="error=\"invalid_token\"", error="insufficient_scope""#]
                [..],
            Some(OAuthRefusal::InsufficientScope),
        ),
        (
            &[
                r#"Basic realm="x", error="invalid_token""#,
                r#"Bearer error="insufficient_scope""#,
            ][..],
            Some(OAuthRefusal::InsufficientScope),
        ),
        (&[r#"Basic error="invalid_token""#][..], None),
        (&[r#"Bearer realm="GitLab""#][..], None),
        (&[r#"Bearer error="unknown_code""#][..], None),
        (&[r#"Bearer error="insufficient_scope"#][..], None),
    ] {
        assert_eq!(
            OAuthRefusal::of_answer(&challenges(values), ""),
            refusal,
            "{values:?}"
        );
    }
}

/// The challenge is the header the RFC defines; the body only stands in for it.
#[test]
fn the_challenge_comes_before_the_body() {
    assert_eq!(
        OAuthRefusal::of_answer(
            &challenges(&[r#"Bearer error="invalid_token""#]),
            r#"{"error":"insufficient_scope"}"#
        ),
        Some(OAuthRefusal::InvalidToken)
    );
}
