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
    {
        let files = Some(vec!["https://other.example.org/design/key".into()]);
        let config = FigmaConfig {
            token: "token".into(),
            files,
        };
        let result = check(
            &ProviderContext::default(),
            &config,
            |config| FigmaApi::at(&server.url, config),
            async { local_tabs::Outcome::Absent },
        )
        .await;
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
        (
            200,
            "{}",
            Some(ProviderProblem::Failed),
            IdentityEvidence::Unnamed,
        ),
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
        let result = check(
            &ProviderContext::default(),
            &config,
            |config| FigmaApi::at(&server.url, config),
            async { local_tabs::Outcome::Absent },
        )
        .await;
        assert_eq!(result.outcome.problem(), problem);
        assert_eq!(
            matches!(result.outcome, CheckOutcome::Available),
            problem.is_none()
        );
        assert_eq!(result.identity, identity);
        assert_eq!(server.requests()[0].path(), "/v1/me");
    }
}

#[tokio::test]
async fn a_tabs_only_connection_accepts_open_tabs_or_a_recent_book_for_the_verified_account() {
    use crate::{
        model::ConnectionId,
        platform::private_file::open_folder,
        provider::connection::{Connection, ConnectionLocation},
    };

    let config = FigmaConfig {
        token: "example-token".into(),
        files: None,
    };
    let server = TestServer::start(|_| Reply::json(200, r#"{"id":"1001","handle":"ada"}"#));
    let temp = tempfile::tempdir().unwrap();
    let tab_path = temp.path().join("settings.json");
    let connection = Connection::new(
        ConnectionId::new(Source::Figma, "design"),
        "Design",
        ConnectionLocation::Local,
        Default::default(),
    );
    let mut context = ProviderContext::for_connection(&connection, &[]);
    context.set_state_dir(Some(temp.path().to_path_buf()));

    let disk = open_folder(temp.path()).unwrap();
    let write_book = |id: &str, days_old: i64| {
        let principal = RemoteIdentity {
            id: id.into(),
            username: "ada".into(),
        };
        let book = file_book::path(&context, &principal).unwrap();
        let bytes = serde_json::to_vec(&serde_json::json!({
            "version": 1,
            "files": { "Remembered": { "last_tab": chrono::Utc::now() - chrono::TimeDelta::days(days_old) } },
        })).unwrap();
        disk.replace(book.file_name().unwrap().to_str().unwrap(), &bytes)
            .unwrap();
    };
    write_book("9001", 1);
    write_book("1001", 61);
    for (user, expected) in [
        ("9001", Some(ProviderProblem::NotConfigured)),
        ("1001", None),
    ] {
        std::fs::write(&tab_path, format!(r#"{{"lastOpenWindow":{{"tabs":[{{"path":"/design/ExampleFile", "userId":"{user}"}}]}}}}"#)).unwrap();
        let result = check(
            &context,
            &config,
            |config| FigmaApi::at(&server.url, config),
            std::future::ready(local_tabs::read(&tab_path)),
        )
        .await;
        assert_eq!(result.outcome.problem(), expected);
        assert!(
            matches!(result.identity, IdentityEvidence::Verified(ref identity) if identity.id == "1001")
        );
    }

    std::fs::remove_file(&tab_path).unwrap();
    write_book("1001", 1);
    let result = check(
        &context,
        &config,
        |config| FigmaApi::at(&server.url, config),
        std::future::ready(local_tabs::read(&tab_path)),
    )
    .await;
    assert!(matches!(result.outcome, CheckOutcome::Available));
    assert!(
        server
            .requests()
            .iter()
            .all(|request| request.path() == "/v1/me")
    );
}
