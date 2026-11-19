use reqwest::StatusCode;

use super::{ApiError, remote_message};

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
