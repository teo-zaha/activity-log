use serde_json::{Value, json};

use crate::{
    model::{ConnectionId, LogLevel, ProviderProblem},
    provider::{
        remote_api::test_server::{Reply, TestServer},
        source_failure::problem_of,
    },
};

use super::*;

const START: &str = "2026-11-23T08:00:00Z";
const END: &str = "2026-11-23T10:00:00Z";
const OLDER: &str = "2026-11-23T07:59:59Z";
const MINE: &str = "1001";
const OTHER: &str = "2002";

fn options() -> CollectOptions {
    CollectOptions {
        start: Timestamp::from(START).utc(),
        end: Timestamp::from(END).utc(),
    }
}

fn config(files: &[&str]) -> FigmaConfig {
    FigmaConfig {
        token: "example-token".into(),
        files: Some(files.iter().map(|file| (*file).into()).collect()),
    }
}

async fn collect(server: &TestServer, files: &[&str]) -> Result<Vec<FigmaEntry>> {
    let config = config(files);
    let scope = FigmaScope::of(&config).unwrap();
    let api = FigmaApi::at(&server.url, &config).unwrap();
    collect_files(&options(), &api, scope, MINE).await
}

fn reply(body: Value) -> Reply {
    Reply::json(200, body.to_string())
}

fn meta(name: &str, editor: &str, last_touched: &str, url: &str) -> Reply {
    reply(json!({"file": {
        "name": name, "editorType": editor, "last_touched_at": last_touched, "url": url
    }}))
}

fn version(id: &str, user: &str, time: &str, label: Option<&str>) -> Value {
    json!({"id": id, "user": {"id": user}, "created_at": time, "label": label, "description": " "})
}

fn comment(id: &str, user: &str, time: &str) -> Value {
    json!({
        "id": id, "user": {"id": user}, "created_at": time,
        "message": "Checked the title", "client_meta": {"x":1,"y":2}
    })
}

#[tokio::test]
async fn keeps_the_owners_entries_in_range_and_resolves_replies_against_root_comments() {
    let server = TestServer::start(|request| match request.path() {
        "/v1/files/ZRecent/meta" => meta(
            "Checkout",
            "figma",
            END,
            "https://www.figma.com/design/ZRecent/Checkout?node-id=9%3A9&mode=design#history",
        ),
        "/v1/files/ZRecent/versions" => reply(json!({"versions": [
            version("16", MINE, END, Some("End boundary")),
            version("15", OTHER, "2026-11-23T09:00:00Z", Some("Another author's version")),
            version("14", MINE, "2026-11-23T08:05:00Z", Some("\t")),
            version("13", MINE, START, Some("Checkout v2")),
        ], "pagination": {}})),
        "/v1/files/ZRecent/comments" => {
            let mut root = comment("10", OTHER, OLDER);
            root["parent_id"] = json!("");
            root["client_meta"] = json!({"node_id": "3:4"});
            let mut reply_row = comment("12", MINE, "2026-11-23T08:15:00.123Z");
            reply_row["parent_id"] = json!("10");
            reply_row["message"] = json!("  ");
            reply_row["client_meta"] = Value::Null;
            let mut resolved = comment("11", MINE, "2026-11-23T08:15:00.123Z");
            resolved["resolved_at"] = json!(END);
            resolved.as_object_mut().unwrap().remove("client_meta");
            reply(json!({"comments": [
                reply_row, comment("40", MINE, END), resolved, root,
                comment("41", OTHER, START), comment("42", MINE, OLDER),
            ]}))
        }
        "/v1/files/AOld/meta" => meta(
            "Draft",
            "unfamiliar",
            OLDER,
            "https://different.example.org/file/AOld",
        ),
        "/v1/files/AOld/comments" => reply(json!({"comments":[comment("50", MINE, START)]})),
        _ => Reply::json(500, "{}"),
    });

    let entries = collect(&server, &["ZRecent", "AOld"]).await.unwrap();
    let keys: Vec<_> = entries.iter().map(|entry| entry.id.as_str()).collect();
    assert_eq!(
        keys,
        [
            "ZRecent:version:13",
            "ZRecent:version:14",
            "ZRecent:comment:11",
            "ZRecent:comment:12",
            "AOld:comment:50",
        ]
    );
    assert!(matches!(&entries[0].activity, FigmaActivity::VersionSaved {
        version_id, label, description: None
    } if version_id == "13" && label == "Checkout v2"));
    assert_eq!(entries[0].timestamp, START);
    assert_eq!(entries[0].editor, Some(FigmaEditor::Figma));
    let base_url = "https://www.figma.com/design/ZRecent/Checkout";
    assert_eq!(entries[0].file_url, base_url);
    assert_eq!(entries[1].file_url, base_url);
    assert!(
        matches!(&entries[1].activity, FigmaActivity::Edited { version_id } if version_id == "14")
    );
    assert!(matches!(
        &entries[2].activity,
        FigmaActivity::Commented {
            resolved: true,
            node_id: None,
            ..
        }
    ));
    assert!(matches!(&entries[3].activity, FigmaActivity::Replied {
        parent_id, message: None, node_id: Some(node), ..
    } if parent_id == "10" && node == "3:4"));
    assert_eq!(entries[3].timestamp, "2026-11-23T08:15:00.123Z");
    assert_eq!(
        entries[3].file_url,
        "https://www.figma.com/design/ZRecent/Checkout?node-id=3%3A4"
    );
    assert_eq!(entries[4].editor, None);
    assert_eq!(entries[4].file_url, "https://www.figma.com/file/AOld");

    let requests = server.requests();
    let old_paths: Vec<_> = requests
        .iter()
        .map(|req| req.path())
        .filter(|path| path.contains("/AOld/"))
        .collect();
    assert_eq!(
        old_paths,
        ["/v1/files/AOld/meta", "/v1/files/AOld/comments"]
    );
    assert!(requests.iter().all(|req| req.path() != "/v1/me"));
}

#[tokio::test]
async fn file_warnings_are_grouped_by_kind_and_name_the_unexpected_origin() {
    let server = TestServer::start(|request| match request.path() {
        "/v1/files/Keep/meta" => meta("Keep", "unfamiliar", OLDER, "https://example.org/file/Keep"),
        "/v1/files/Keep/comments" => reply(json!({"comments":[comment("11", MINE, START)]})),
        _ => Reply::json(404, "{}"),
    });

    let id = ConnectionId::new(Source::Figma, "warning-test");
    let entries = diagnostics::in_connection(
        id.clone(),
        collect(&server, &["Keep", "https://www.figma.com/file/Keep"]),
    )
    .await
    .unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].file_key, "Keep");

    let lines: Vec<_> = diagnostics::entries()
        .into_iter()
        .filter(|line| line.scope == id.to_string() && line.level == LogLevel::Warn)
        .map(|line| line.message)
        .collect();
    assert_eq!(lines.len(), 3, "{lines:?}");
    assert!(
        lines
            .iter()
            .any(|line| line.contains("https://example.org"))
    );
}

#[tokio::test]
async fn a_file_request_failure_keeps_the_original_position_and_remote_problem() {
    for (status, expected) in [
        (403, Some(ProviderProblem::Refused)),
        (404, None),
        (429, Some(ProviderProblem::Throttled)),
    ] {
        let server = TestServer::start(move |request| match request.path() {
            "/v1/files/Keep/meta" => {
                meta("Keep", "figma", OLDER, "https://www.figma.com/file/Keep")
            }
            "/v1/files/Keep/comments" => reply(json!({"comments":[comment("11", MINE, START)]})),
            "/v1/files/Unreadable/meta" => meta(
                "File",
                "figma",
                END,
                "https://www.figma.com/file/Unreadable",
            ),
            "/v1/files/Unreadable/versions" => reply(json!({
                "versions": [version("13", MINE, START, None)], "pagination": {}
            })),
            _ => Reply::json(status, r#"{"err":"File unavailable"}"#),
        });
        let error = collect(
            &server,
            &["Keep", "https://www.figma.com/file/Keep", "Unreadable"],
        )
        .await
        .unwrap_err();
        assert_eq!(problem_of(&error), expected);
        assert!(matches!(
            error.downcast_ref::<FigmaCollectError>(),
            Some(FigmaCollectError::FileRequest {
                position: 3,
                failure: RemoteFailure { error: ApiError::Status { status: got, .. }, .. },
            }) if got.as_u16() == status
        ));
    }
}

#[tokio::test]
async fn collection_requires_a_verified_identity() {
    for identity in [
        IdentityEvidence::None,
        IdentityEvidence::Unnamed,
        IdentityEvidence::Denied,
    ] {
        let mut context = ProviderContext::default();
        context.set_identity(identity);
        let error = collect_figma(&options(), &context).await.unwrap_err();
        assert!(matches!(
            error.downcast_ref::<FigmaCollectError>(),
            Some(FigmaCollectError::NoIdentity)
        ));
    }
}
