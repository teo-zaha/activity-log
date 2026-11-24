use serde_json::{Value, json};

use super::{ApiError, QueryAnswer, QueryErrorKind, QueryPathSegment, query_data};
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
        refusal.kinds().collect::<Vec<_>>(),
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

#[test]
fn graphql_keeps_typed_paths_even_when_data_was_returned() {
    let path = json!(["nodes", 0, "defaultBranchRef", "target", "history"]);
    let data = json!({"nodes": [false, 42, null]});
    let body = json!({
        "data": data,
        "errors": [{"type": "FORBIDDEN", "path": path}],
    });
    let ApiError::Query(refusal) = query_data::<Value>(body.clone(), &[]).unwrap_err() else {
        panic!("strict queries must keep the refusal");
    };
    assert_eq!(
        refusal.errors()[0].path.as_deref(),
        Some(
            [
                QueryPathSegment::Field("nodes".into()),
                QueryPathSegment::Index(0),
                QueryPathSegment::Field("defaultBranchRef".into()),
                QueryPathSegment::Field("target".into()),
                QueryPathSegment::Field("history".into()),
            ]
            .as_slice(),
        )
    );
    let (partial, partial_refusal) = QueryAnswer::read(body, &[]).unwrap().into_parts();
    assert_eq!(partial, Some(data));
    assert_eq!(partial_refusal, Some(refusal));
}

#[test]
fn missing_or_malformed_graphql_paths_supply_no_attribution() {
    for error in [
        json!({"type": "FORBIDDEN"}),
        json!({"type": "FORBIDDEN", "path": ["nodes", -1]}),
        json!({"type": "FORBIDDEN", "path": ["nodes", null]}),
    ] {
        let answer = QueryAnswer::read(json!({"errors": [error]}), &[]).unwrap();
        let (_, refusal) = answer.into_parts();
        assert_eq!(refusal.unwrap().errors()[0].path, None);
    }
}
