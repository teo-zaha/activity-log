use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::{
    model::{ConnectionId, LogLevel, ProviderProblem},
    provider::{
        remote_api::test_server::{Reply, TestServer},
        source_failure::{ClassifiedProblem, problem_of},
    },
};

use super::*;

const START: &str = "2020-11-23T08:00:00Z";
const END: &str = "2020-11-23T10:00:00Z";
const OLDER: &str = "2020-11-23T07:59:59Z";
const MINE: &str = "1001";
const OTHER: &str = "2002";

// Open live check: verify all three v2 envelopes, top-level-only team folders, pagination,
// rate tier, and responses without folders:read, without nested folders, and for deleted
// or invite-only folders. Check /meta, /versions and /comments for each listed editor,
// whether comments change last_modified, and request counts and duration. See ADR 0008.

#[tokio::test]
async fn historical_ranges_discover_missing_teams_and_recent_runs_skip_unchanged_files() {
    let server = TestServer::start(|request| match request.path() {
        "/v2/teams/100001/folders" => reply(json!({
            "name": "Design",
            "folders": [{"id":"200001","name":"Examples","parent_folder_id":null}]
        })),
        "/v2/folders/200001/folders" => reply(json!({
            "name": "Examples",
            "folders": [{"id":"200002","name":"Drafts","parent_folder_id":"200001"}]
        })),
        "/v2/folders/200002/folders" => reply(json!({"name":"Drafts","folders":[]})),
        "/v2/folders/200001/files" => reply(json!({"name":"Examples","files":[]})),
        "/v2/folders/200002/files" => reply(json!({
            "name": "Drafts",
            "files": [{"key":"TeamFile","name":"Draft","last_modified":START}]
        })),
        "/v1/files/TeamFile/meta" => meta(
            "Draft",
            "figma",
            OLDER,
            "https://www.figma.com/design/TeamFile/Draft",
        ),
        "/v1/files/TeamFile/comments" => reply(json!({"comments":[comment("50", MINE, START)]})),
        _ => Reply::json(500, "{}"),
    });
    let mut config = config(&[]);
    config.teams = Some(vec!["100001".into()]);
    let api = FigmaApi::at(&server.url, &config).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("figma-design.files.json");
    let state = FigmaCollection::default();
    let run = state.run(None, Some(&path));
    let tabs = tabs_for(&[]);
    let now = Utc::now();
    let recent = CollectOptions {
        start: now - TimeDelta::hours(1),
        end: now,
    };
    let fetch = |range, run| {
        collect_files(
            range,
            &api,
            FigmaScope::of(&config).unwrap(),
            MINE,
            run,
            &tabs,
        )
    };

    let historical = options();
    assert_eq!(
        fetch(&historical, &run).await.unwrap()[0].id,
        "TeamFile:comment:50"
    );
    let historical_run = state.run(None, Some(&path));
    assert_eq!(fetch(&historical, &historical_run).await.unwrap().len(), 1);

    let recent_run = state.run(None, Some(&path));
    assert!(fetch(&recent, &recent_run).await.unwrap().is_empty());
    assert!(fetch(&recent, &recent_run).await.unwrap().is_empty());
    let requests = server.requests();
    let listing_paths: Vec<_> = requests
        .iter()
        .map(|request| request.path())
        .filter(|path| path.starts_with("/v2"))
        .collect();
    assert_eq!(
        listing_paths,
        [
            "/v2/teams/100001/folders",
            "/v2/folders/200001/folders",
            "/v2/folders/200002/folders",
            "/v2/folders/200001/files",
            "/v2/folders/200002/files",
            "/v2/folders/200001/files",
            "/v2/folders/200002/files",
        ]
    );
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.path() == "/v1/files/TeamFile/comments")
            .count(),
        2
    );
}

#[tokio::test]
async fn partial_folder_discovery_keeps_the_previous_complete_set() {
    use super::{
        file_book::TeamFolders,
        team_discovery::{TeamDiscoveryError, TeamFailure},
    };
    use crate::model::figma::FigmaFolderId;

    let mut config = config(&[]);
    config.teams = Some(vec!["100001".into()]);
    let scope = FigmaScope::of(&config).unwrap();
    let team = scope.teams[0].1.clone();
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("figma-design.files.json");
    let state = FigmaCollection::default();
    let run = state.run(None, Some(&path));
    let previous = Utc::now() - TimeDelta::days(2);
    let folder: FigmaFolderId = serde_json::from_value(json!("399999")).unwrap();
    run.book
        .remember_team(
            team,
            Some(TeamFolders {
                discovered_at: previous,
                folders: [folder].into(),
            }),
            BTreeMap::new(),
            previous,
        )
        .await
        .unwrap();
    let before = std::fs::read(&path).unwrap();

    let server = TestServer::start(|request| match request.path() {
        "/v2/teams/100001/folders" => reply(json!({
            "name": "Design",
            "folders":[{"id":"200001","name":"Root","parent_folder_id":null}]
        })),
        "/v2/folders/200001/folders" => Reply::json(403, r#"{"status":403,"err":"No access"}"#),
        _ => Reply::json(500, "{}"),
    });
    let api = FigmaApi::at(&server.url, &config).unwrap();
    let error = team_discovery::refresh(&api, &run.book, &scope.teams, true)
        .await
        .unwrap_err();
    assert!(matches!(
        error.downcast_ref::<TeamDiscoveryError>(),
        Some(TeamDiscoveryError { position: 1, cause: TeamFailure::Request(failure) })
            if matches!(failure.error, ApiError::Status { status: reqwest::StatusCode::FORBIDDEN, .. })
    ));
    assert_eq!(
        problem_of(&error),
        Some(ClassifiedProblem::plain(ProviderProblem::Refused))
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(server.requests().len(), 2);
}

#[tokio::test]
async fn pagination_on_any_folder_endpoint_refuses_the_listing_without_saving_it() {
    use super::team_discovery::{TeamDiscoveryError, TeamFailure};

    for (endpoint, requests, header) in [
        ("/v2/teams/100001/folders", 1, None),
        ("/v2/folders/200001/folders", 2, None),
        (
            "/v2/folders/200001/files",
            3,
            Some(("x-next-cursor", "following")),
        ),
        ("/v2/folders/200001/files", 3, Some(("x-next-page", "2"))),
        (
            "/v2/folders/200001/files",
            3,
            Some(("link", r#"<https://example.org/next>; REL="NeXt prev""#)),
        ),
    ] {
        let server = TestServer::start(move |request| {
            let mut body = match request.path() {
                "/v2/teams/100001/folders" => json!({
                    "name": "Design",
                    "folders":[{"id":"200001","name":"Root","parent_folder_id":null}]
                }),
                "/v2/folders/200001/folders" => json!({"name":"Root","folders":[]}),
                "/v2/folders/200001/files" => json!({"name":"Root","files":[]}),
                _ => panic!("unexpected folder request"),
            };
            if request.path() == endpoint {
                if let Some((name, value)) = header {
                    return reply(body).with_header(name, value);
                }
                match requests {
                    1 => body["pagination"] = json!({"next_page": "https://example.org/next"}),
                    _ => body["cursor"] = json!("following"),
                }
            }
            reply(body)
        });
        let mut config = config(&[]);
        config.teams = Some(vec!["100001".into()]);
        let scope = FigmaScope::of(&config).unwrap();
        let api = FigmaApi::at(&server.url, &config).unwrap();
        let state = FigmaCollection::default();
        let run = state.run(None, None);
        let error = team_discovery::refresh(&api, &run.book, &scope.teams, true)
            .await
            .unwrap_err();
        assert!(matches!(
            error.downcast_ref::<TeamDiscoveryError>(),
            Some(TeamDiscoveryError {
                position: 1,
                cause: TeamFailure::Paginated
            })
        ));
        assert_eq!(server.requests().len(), requests);
        assert!(
            run.book
                .team_folders(scope.teams[0].1.clone(), Utc::now())
                .await
                .unwrap()
                .is_none()
        );
    }
}

#[tokio::test]
async fn an_unreadable_saved_folder_triggers_one_rediscovery() {
    use super::{
        file_book::TeamFolders,
        team_discovery::{TeamDiscoveryError, TeamFailure},
    };
    use crate::model::figma::FigmaFolderId;

    for still_listed in [false, true] {
        let server = TestServer::start(move |request| match request.path() {
            "/v2/teams/100001/folders" if still_listed => reply(json!({
                "name":"Design","folders":[{"id":"200001","name":"Root","parent_folder_id":null}]
            })),
            "/v2/teams/100001/folders" | "/v2/folders/200001/folders" => {
                reply(json!({"name":"Root","folders":[]}))
            }
            "/v2/folders/200001/files" => Reply::json(404, r#"{"err":"Folder unavailable"}"#),
            _ => panic!("unexpected folder request"),
        });
        let mut config = config(&[]);
        config.teams = Some(vec!["100001".into()]);
        let scope = FigmaScope::of(&config).unwrap();
        let api = FigmaApi::at(&server.url, &config).unwrap();
        let state = FigmaCollection::default();
        let run = state.run(None, None);
        let before = Utc::now() - TimeDelta::minutes(5);
        let folder: FigmaFolderId = serde_json::from_value(json!("200001")).unwrap();
        let team = scope.teams[0].1.clone();
        run.book
            .remember_team(
                team.clone(),
                Some(TeamFolders {
                    folders: [folder.clone()].into(),
                    discovered_at: before,
                }),
                BTreeMap::new(),
                before,
            )
            .await
            .unwrap();

        let result = team_discovery::refresh(&api, &run.book, &scope.teams, true).await;
        let stored = run
            .book
            .team_folders(team, Utc::now())
            .await
            .unwrap()
            .unwrap();
        let requests = server.requests();
        let paths: Vec<_> = requests.iter().map(|request| request.path()).collect();
        if still_listed {
            let error = result.unwrap_err();
            assert!(matches!(
                error.downcast_ref::<TeamDiscoveryError>(),
                Some(TeamDiscoveryError { position: 1, cause: TeamFailure::Request(failure) })
                    if matches!(failure.error, ApiError::Status { status: reqwest::StatusCode::NOT_FOUND, .. })
            ));
            assert_eq!(stored.discovered_at, before);
            assert_eq!(stored.folders, [folder].into());
            assert_eq!(
                paths,
                [
                    "/v2/folders/200001/files",
                    "/v2/teams/100001/folders",
                    "/v2/folders/200001/folders",
                    "/v2/folders/200001/files",
                ]
            );
        } else {
            result.unwrap();
            assert!(stored.discovered_at > before);
            assert!(stored.folders.is_empty());
            assert_eq!(
                paths,
                ["/v2/folders/200001/files", "/v2/teams/100001/folders"]
            );
        }
    }
}

#[tokio::test]
async fn invalid_team_file_keys_are_skipped_with_one_warning() {
    let server = TestServer::start(|request| match request.path() {
        "/v2/teams/100001/folders" => reply(json!({
            "name":"Design","folders":[{"id":"200001","name":"Root","parent_folder_id":null}]
        })),
        "/v2/folders/200001/folders" => reply(json!({"name":"Root","folders":[]})),
        "/v2/folders/200001/files" => reply(json!({"name":"Root","files":[
            {"key":"../Invalid","last_modified":START},
            {"key":null,"last_modified":START},
            {"key":"TeamFile","last_modified":START},
        ]})),
        "/v1/files/TeamFile/meta" => meta(
            "Draft",
            "figma",
            OLDER,
            "https://www.figma.com/file/TeamFile",
        ),
        "/v1/files/TeamFile/comments" => reply(json!({"comments":[comment("50", MINE, START)]})),
        _ => panic!("unexpected file request"),
    });
    let mut config = config(&[]);
    config.teams = Some(vec!["100001".into()]);
    let scope = FigmaScope::of(&config).unwrap();
    let api = FigmaApi::at(&server.url, &config).unwrap();
    let state = FigmaCollection::default();
    let id = ConnectionId::new(Source::Figma, "invalid-team-file-keys");
    let run = state.run(Some(&id), None);
    let entries = diagnostics::in_connection(
        id.clone(),
        collect_files(&options(), &api, scope, MINE, &run, &tabs_for(&[])),
    )
    .await
    .unwrap();
    assert_eq!(entries[0].id, "TeamFile:comment:50");
    let warnings: Vec<_> = diagnostics::entries()
        .into_iter()
        .filter(|line| line.scope == id.to_string() && line.level == LogLevel::Warn)
        .collect();
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].message.contains("2 affected"));
}

#[tokio::test]
async fn inaccessible_and_throttled_teams_fail_collection_with_the_original_list_position() {
    use super::team_discovery::{TeamDiscoveryError, TeamFailure};

    for (status, problem) in [(404, None), (429, Some(ProviderProblem::Throttled))] {
        let server = TestServer::start(move |request| match request.path() {
            "/v2/teams/100001/folders" => reply(json!({"name":"Design","folders":[]})),
            "/v2/teams/200002/folders" => Reply::json(status, r#"{"err":"Test listing error"}"#),
            _ => Reply::json(500, "{}"),
        });
        let mut config = config(&[]);
        config.teams = Some(vec!["100001".into(), "100001".into(), "200002".into()]);
        let api = FigmaApi::at(&server.url, &config).unwrap();
        let state = FigmaCollection::default();
        let run = state.run(None, None);
        let now = Utc::now();
        let range = CollectOptions {
            start: now - TimeDelta::minutes(15),
            end: now,
        };
        let error = collect_files(
            &range,
            &api,
            FigmaScope::of(&config).unwrap(),
            MINE,
            &run,
            &tabs_for(&[]),
        )
        .await
        .unwrap_err();
        assert!(matches!(
            error.downcast_ref::<TeamDiscoveryError>(),
            Some(TeamDiscoveryError { position: 3, cause: TeamFailure::Request(failure) })
                if matches!(failure.error, ApiError::Status { status: actual, .. } if actual.as_u16() == status)
        ));
        assert_eq!(problem_of(&error), problem.map(ClassifiedProblem::plain));
    }
}

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
        teams: None,
    }
}

async fn collect(server: &TestServer, files: &[&str]) -> Result<Vec<FigmaEntry>> {
    let config = config(files);
    let scope = FigmaScope::of(&config).unwrap();
    let api = FigmaApi::at(&server.url, &config).unwrap();
    let state = FigmaCollection::default();
    let run = state.run(None, None);
    let tabs = TabSnapshot {
        outcome: local_tabs::Outcome::Absent,
        seen_at: Utc::now(),
    };
    collect_files(&options(), &api, scope, MINE, &run, &tabs).await
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
            version("15", OTHER, "2020-11-23T09:00:00Z", Some("Another author's version")),
            version("14", MINE, "2020-11-23T08:05:00Z", Some("\t")),
            version("13", MINE, START, Some("Checkout v2")),
        ], "pagination": {}})),
        "/v1/files/ZRecent/comments" => {
            let mut root = comment("10", OTHER, OLDER);
            root["parent_id"] = json!("");
            root["client_meta"] = json!({"node_id": "3:4"});
            let mut reply_row = comment("12", MINE, "2020-11-23T08:15:00.123Z");
            reply_row["parent_id"] = json!("10");
            reply_row["message"] = json!("  ");
            reply_row["client_meta"] = Value::Null;
            let mut resolved = comment("11", MINE, "2020-11-23T08:15:00.123Z");
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
    assert_eq!(entries[3].timestamp, "2020-11-23T08:15:00.123Z");
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
        assert_eq!(problem_of(&error), expected.map(ClassifiedProblem::plain));
        assert!(matches!(
            error.downcast_ref::<FigmaCollectError>(),
            Some(FigmaCollectError::FileRequest {
                file: FilePlace::Pasted(3),
                failure: RemoteFailure { error: ApiError::Status { status: got, .. }, .. },
            }) if got.as_u16() == status
        ));
    }
}

#[tokio::test]
async fn collection_requires_a_verified_identity() {
    for identity in [IdentityEvidence::None, IdentityEvidence::Denied] {
        let mut context = ProviderContext::default();
        context.set_identity(identity);
        let error = collect_figma(&options(), &context).await.unwrap_err();
        assert!(matches!(
            error.downcast_ref::<FigmaCollectError>(),
            Some(FigmaCollectError::NoIdentity)
        ));
    }
}

fn tabs_for(files: &[&str]) -> TabSnapshot {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("settings.json");
    let tabs: Vec<_> = files
        .iter()
        .map(|file| json!({"path": format!("/design/{file}"), "userId": MINE}))
        .collect();
    std::fs::write(&path, json!({"lastOpenWindow":{"tabs":tabs}}).to_string()).unwrap();
    TabSnapshot {
        outcome: local_tabs::read(&path),
        seen_at: Utc::now(),
    }
}

#[tokio::test]
async fn discovered_denials_share_one_identity_check_and_one_warning() {
    let connection = ConnectionId::new(Source::Figma, "discovered-denial-test");
    diagnostics::in_connection(connection.clone(), async {
        let server = TestServer::start(|req| match req.path() {
            "/v1/me" => Reply::json(200, r#"{"id":"1001","handle":"ada"}"#),
            "/v1/files/Gone/meta" => Reply::json(404, "{}"),
            _ => Reply::json(403, "{}"),
        });
        let config = config(&[]);
        let api = FigmaApi::at(&server.url, &config).unwrap();
        let state = FigmaCollection::default();
        let run = state.run(Some(&connection), None);
        let tabs = tabs_for(&["First", "Second", "Gone"]);
        for _ in 0..2 {
            let entries = collect_files(
                &options(),
                &api,
                FigmaScope::of(&config).unwrap(),
                MINE,
                &run,
                &tabs,
            )
            .await
            .unwrap();
            assert!(entries.is_empty());
        }
        let requests = server.requests();
        assert_eq!(
            requests.iter().filter(|req| req.path() == "/v1/me").count(),
            1
        );
        assert_eq!(requests.len(), 4);
    })
    .await;
    assert_eq!(
        diagnostics::entries()
            .iter()
            .filter(|line| { line.scope == connection.to_string() && line.level == LogLevel::Warn })
            .count(),
        1
    );
}

#[tokio::test]
async fn a_busy_discovered_403_fails_the_chunk_without_an_identity_probe() {
    let server =
        TestServer::start(|_| Reply::json(403, "{}").with_header("X-RateLimit-Remaining", "0"));
    let config = config(&[]);
    let api = FigmaApi::at(&server.url, &config).unwrap();
    let state = FigmaCollection::default();
    let run = state.run(None, None);
    let error = collect_files(
        &options(),
        &api,
        FigmaScope::of(&config).unwrap(),
        MINE,
        &run,
        &tabs_for(&["Busy"]),
    )
    .await
    .unwrap_err();
    assert_eq!(
        problem_of(&error),
        Some(ClassifiedProblem::plain(ProviderProblem::Throttled))
    );
    assert!(matches!(
        error.downcast_ref::<FigmaCollectError>(),
        Some(FigmaCollectError::FileRequest {
            file: FilePlace::Discovered(key),
            ..
        }) if key.as_str() == "Busy"
    ));
    assert_eq!(server.requests().len(), 1);
    assert_eq!(server.requests()[0].path(), "/v1/files/Busy/meta");
}

#[tokio::test]
async fn a_scope_refusal_keeps_the_discovered_file_and_skips_the_identity_probe() {
    let server = TestServer::start(|_| Reply::json(403, r#"{"error":"insufficient_scope"}"#));
    let config = config(&[]);
    let api = FigmaApi::at(&server.url, &config).unwrap();
    let state = FigmaCollection::default();
    let run = state.run(None, None);
    let tabs = tabs_for(&["Example"]);
    for _ in 0..2 {
        let error = collect_files(
            &options(),
            &api,
            FigmaScope::of(&config).unwrap(),
            MINE,
            &run,
            &tabs,
        )
        .await
        .unwrap_err();
        assert_eq!(
            problem_of(&error),
            Some(ClassifiedProblem::plain(ProviderProblem::ScopeMissing))
        );
    }
    assert_eq!(
        server
            .requests()
            .iter()
            .map(|request| request.path())
            .collect::<Vec<_>>(),
        ["/v1/files/Example/meta"; 2]
    );
}

#[tokio::test]
async fn sightings_are_saved_before_a_request_and_contributions_before_returning_entries() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("figma-design-0123456789abcdef.files.json");
    let seen = path.clone();
    let server = TestServer::start(move |req| match req.path() {
        "/v1/files/Example/meta" if seen.exists() => meta(
            "Example",
            "figma",
            OLDER,
            "https://www.figma.com/file/Example",
        ),
        "/v1/files/Example/comments" => reply(json!({"comments":[comment("11", MINE, START)]})),
        _ => Reply::json(500, "{}"),
    });
    let config = config(&[]);
    let api = FigmaApi::at(&server.url, &config).unwrap();
    let state = FigmaCollection::default();
    let run = state.run(None, Some(&path));
    let entries = collect_files(
        &options(),
        &api,
        FigmaScope::of(&config).unwrap(),
        MINE,
        &run,
        &tabs_for(&["Example"]),
    )
    .await
    .unwrap();
    assert_eq!(entries.len(), 1);
    let saved = serde_json::from_slice::<Value>(&std::fs::read(path).unwrap()).unwrap();
    assert!(saved["files"]["Example"]["last_contribution"].is_string());
    assert_eq!(server.requests().len(), 2);
}
