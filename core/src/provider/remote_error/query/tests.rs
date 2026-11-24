use serde_json::{Value, json};

use super::{ApiError, QueryErrorKind, query_data};
use crate::model::ProviderProblem;

#[test]
fn graphql_errors_refuse_the_whole_answer_and_keep_each_code() {
    let secret = "example-token";
    let body = json!({
        "data": {"viewer": {"login": "jdoe"}},
        "errors": [
            {"type": "FORBIDDEN", "message": "Cannot access acme/widgets"},
            {"type": "RATE_LIMITED", "message": format!("Limit for {secret} at https://user:pass@example.org/graphql?private=yes")},
            {"type": "NOT_FOUND"},
            {"type": "INSUFFICIENT_SCOPES"},
            {"extensions": {"code": "undefinedField"}},
            {"message": "RATE_LIMITED"}
        ],
    });

    let error = query_data::<Value>(body, &[secret]).unwrap_err();
    let ApiError::Query(refusal) = &error else {
        panic!("expected a typed query refusal, got {error:?}");
    };
    assert_eq!(
        refusal.kinds(),
        [
            QueryErrorKind::Forbidden,
            QueryErrorKind::RateLimited,
            QueryErrorKind::NotFound,
            QueryErrorKind::InsufficientScopes,
            QueryErrorKind::UndefinedField,
            QueryErrorKind::Other,
        ]
    );
    assert_eq!(error.problem(), Some(ProviderProblem::Throttled));
    let detail = error.message("GitHub");
    assert!(detail.contains("[redacted]"));
    assert!(detail.contains("https://example.org/graphql"));
    for private in [secret, "user:pass", "private=yes"] {
        assert!(!detail.contains(private));
    }
}

#[test]
fn graphql_scopes_and_access_codes_are_refusals_but_message_text_is_untagged() {
    for (code, expected) in [
        ("INSUFFICIENT_SCOPES", Some(ProviderProblem::Refused)),
        ("FORBIDDEN", Some(ProviderProblem::Refused)),
        ("NOT_FOUND", None),
        ("unrecognized", None),
    ] {
        let body = json!({"errors": [{"type": code, "message": "RATE_LIMITED"}]});
        let error = query_data::<Value>(body, &[]).unwrap_err();
        assert_eq!(error.problem(), expected, "{code}");
    }
}

#[test]
fn malformed_graphql_envelopes_cannot_be_read_as_complete() {
    for body in [
        json!({"data": {"ok": true}, "errors": []}),
        json!({"data": {"ok": true}, "errors": null}),
        json!({"data": null}),
        json!({"ok": true}),
    ] {
        assert_eq!(query_data::<Value>(body, &[]), Err(ApiError::Decode));
    }
}
