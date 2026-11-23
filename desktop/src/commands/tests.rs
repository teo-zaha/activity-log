use std::{sync::mpsc, time::Duration};

use activity_log_core::{
    diagnostics,
    model::{BackendError, BackendErrorKind},
    settings::{ConfigFile, Precondition},
};
use tauri::{
    ipc::{CallbackFn, InvokeBody},
    test::{INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder, mock_context, noop_assets},
    webview::InvokeRequest,
};

use super::{update_config, web_url, write_export};
use crate::app_state::AppState;

fn labels(path: &std::path::Path) -> Vec<String> {
    let file = ConfigFile::load(path).unwrap();
    let connections = file.connections().connections;
    connections.into_iter().map(|c| c.label).collect()
}

/// Two settings commands arrive together. The second is released as soon as the first has read
/// the file: it must still start from what the first one saves.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn settings_commands_that_arrive_together_both_apply() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(
        &path,
        "[[gitlab]]\nlabel = \"A\"\n\n[[gitlab]]\nlabel = \"B\"\n",
    )
    .unwrap();
    let ids = ConfigFile::load(&path).unwrap().connections();
    let [a, b] = <[_; 2]>::try_from(ids.connections)
        .ok()
        .unwrap()
        .map(|c| c.id);
    let state = AppState::start(
        Some(path.clone()),
        activity_log_core::secrets::Secrets::none(),
    );
    let (first_loaded, first_has_loaded) = mpsc::channel();
    let (second_loaded, second_has_loaded) = mpsc::channel();

    let first = tokio::spawn({
        let state = state.clone();
        async move {
            update_config(&state, Precondition::None, None, move |file| {
                first_loaded.send(()).unwrap();
                // Gives the second command every chance to read the file before this one saves.
                let _ = second_has_loaded.recv_timeout(Duration::from_millis(300));
                file.rename(&a, "A changed")
            })
            .await
        }
    });
    first_has_loaded.recv().unwrap();
    update_config(&state, Precondition::None, None, move |file| {
        let _ = second_loaded.send(());
        file.rename(&b, "B changed")
    })
    .await
    .unwrap();
    let (_, first) = first.await.unwrap().unwrap();

    assert_eq!(labels(&path), ["A changed", "B changed"]);
    // What the app shows is the file as it ends, whichever save answered last.
    let shown = |providers: &activity_log_core::model::ProvidersResponse| {
        providers
            .providers
            .iter()
            .map(|info| info.label.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        shown(&state.host.providers_response()),
        ["A changed", "B changed"]
    );
    assert_eq!(shown(&first.providers_response).len(), 2);
}

/// A file that does not read as a configuration is reported as such, not as a failed save,
/// and is left as it is.
#[tokio::test]
async fn a_save_over_a_file_that_does_not_parse_is_a_bad_config() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let state = AppState::start(
        Some(path.clone()),
        activity_log_core::secrets::Secrets::none(),
    );
    // Broken after the app read it, as another program could.
    std::fs::write(&path, "[[git]\n").unwrap();

    let refused = update_config(&state, Precondition::None, None, |file| {
        file.add(
            &activity_log_core::model::Source::Calendar,
            None,
            activity_log_core::provider::connection::ConnectionLocation::Local,
        )
    })
    .await;

    assert_eq!(refused.unwrap_err().kind, BackendErrorKind::BadConfig);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "[[git]\n");
}

#[test]
fn opens_only_web_links() {
    for link in [
        "https://teams.microsoft.com/l/meetup-join/abc",
        " http://example.org/path?q=1 ",
    ] {
        assert!(web_url(link).is_ok(), "{link}");
    }
    for link in [
        "file:///etc/passwd",
        "javascript:alert(1)",
        "mailto:someone@example.org",
        "ms-teams://join",
        "C:\\Windows\\notepad.exe",
        "/Applications/Calculator.app",
        "not a link",
        "",
    ] {
        assert_eq!(
            web_url(link).unwrap_err().kind,
            BackendErrorKind::InvalidRequest,
            "{link}"
        );
    }
}

/// Runs `command` with `request` as the dashboard does, over IPC: the answer, or what the
/// command rejected with.
fn invoke(
    state: AppState,
    command: &str,
    body: serde_json::Value,
) -> Result<serde_json::Value, serde_json::Value> {
    let app = mock_builder()
        .manage(state)
        .manage(super::ExportTurn::default())
        .invoke_handler(tauri::generate_handler![
            super::get_runtime_state,
            super::collect,
            super::recheck_connection,
            super::save_export,
            super::get_provider_config,
            super::check_provider_config,
            super::save_provider_config,
            super::remove_connection,
            super::add_connection
        ])
        .build(mock_context(noop_assets()))
        .unwrap();
    let webview =
        tauri::WebviewWindowBuilder::<MockRuntime, _>::new(&app, "main", Default::default())
            .build()
            .unwrap();
    get_ipc_response(
        &webview,
        InvokeRequest {
            cmd: command.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: "tauri://localhost".parse().unwrap(),
            body: InvokeBody::Json(body),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        },
    )
    .map(|answer| answer.deserialize().unwrap())
}

fn refusal(state: AppState, command: &str, request: serde_json::Value) -> BackendError {
    let rejected = invoke(state, command, serde_json::json!({ "request": request })).unwrap_err();
    serde_json::from_value(rejected).expect("a command rejects with a BackendError")
}

/// The same requests as the API's tests, refused with the same kinds.
#[test]
fn commands_refuse_with_the_kinds_the_api_answers() {
    let range = |start: &str, end: &str| serde_json::json!({ "start": start, "end": end });
    assert_eq!(
        refusal(
            AppState::start(None, activity_log_core::secrets::Secrets::none()),
            "collect",
            range("2026-03-01", "2026-03-05")
        )
        .kind,
        BackendErrorKind::InvalidRequest
    );
    assert_eq!(
        refusal(
            AppState::start(None, activity_log_core::secrets::Secrets::none()),
            "collect",
            range("1000-01-01T00:00:00Z", "9999-12-31T00:00:00Z")
        )
        .kind,
        BackendErrorKind::InvalidRequest
    );
    assert_eq!(
        refusal(
            AppState::start(None, activity_log_core::secrets::Secrets::none()),
            "recheck_connection",
            serde_json::json!({ "source": "calendar", "name": "other" })
        )
        .kind,
        BackendErrorKind::NotFound
    );
    assert_eq!(
        refusal(
            AppState::start(None, activity_log_core::secrets::Secrets::none()),
            "save_export",
            serde_json::json!({
                "format": "json",
                "start": "2026-03-01T23:00:00Z",
                "end": "2026-03-02T23:00:00Z",
                "timeZone": "Local",
                "entries": [],
            })
        )
        .kind,
        BackendErrorKind::InvalidRequest
    );

    let quitting = AppState::start(None, activity_log_core::secrets::Secrets::none());
    quitting.host.shutdown();
    assert_eq!(
        refusal(
            quitting,
            "collect",
            range("2026-03-01T00:00:00Z", "2026-03-06T00:00:00Z")
        )
        .kind,
        BackendErrorKind::NoData
    );

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "[[calendar]]\nlabel = \"Team\"\n").unwrap();
    let shut_down = AppState::start(Some(path), activity_log_core::secrets::Secrets::none());
    shut_down.host.shutdown();
    assert_eq!(
        refusal(
            shut_down,
            "recheck_connection",
            serde_json::json!({ "source": "calendar", "name": "team" })
        )
        .kind,
        BackendErrorKind::Internal
    );
}

/// Tauri refuses a call it cannot pass to the command (no `request`) with a plain string, not
/// a `BackendError`; the dashboard's transport reads that as an internal failure. The same
/// call with its request reaches the command, so the refusal is Tauri's own.
#[test]
fn tauri_refuses_a_call_it_cannot_decode_with_a_string() {
    let request = serde_json::json!({
        "request": { "start": "2026-03-01T00:00:00Z", "end": "2026-03-06T00:00:00Z" }
    });
    assert!(
        invoke(
            AppState::start(None, activity_log_core::secrets::Secrets::none()),
            "collect",
            request
        )
        .is_ok()
    );

    let rejected = invoke(
        AppState::start(None, activity_log_core::secrets::Secrets::none()),
        "collect",
        serde_json::json!({}),
    )
    .unwrap_err();

    assert!(rejected.is_string(), "{rejected}");
    assert!(serde_json::from_value::<BackendError>(rejected).is_err());
}

/// Without a settings location a save fails as such, and nothing is written.
#[tokio::test]
async fn a_save_without_a_settings_location_writes_nothing() {
    let state = AppState::start(None, activity_log_core::secrets::Secrets::none());

    let refused = update_config(&state, Precondition::None, None, |file| {
        file.rename(&"gitlab/work".parse().unwrap(), "Work")
    })
    .await;

    assert_eq!(refused.unwrap_err().kind, BackendErrorKind::NoLocation);
}

#[tokio::test]
async fn a_busy_settings_file_is_reported_and_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "[[git]]\n").unwrap();
    let held = std::fs::File::create(dir.path().join("config.toml.lock")).unwrap();
    held.lock().unwrap();
    let state = AppState::start(
        Some(path.clone()),
        activity_log_core::secrets::Secrets::none(),
    );

    let refused = update_config(&state, Precondition::None, None, |file| {
        file.rename(&"git/this-computer".parse().unwrap(), "Laptop")
    })
    .await;

    assert_eq!(refused.unwrap_err().kind, BackendErrorKind::Busy);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "[[git]]\n");
    drop(held);
}

#[tokio::test]
async fn config_problems_after_an_edit_reach_the_logs() {
    let _log = crate::tests::DIAGNOSTICS.lock().await;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "[[gitlab]]\nlabel = \"Edited\"\nurll = \"x\"\n").unwrap();
    let state = AppState::start(Some(path), activity_log_core::secrets::Secrets::none());

    let (_, saved) = update_config(&state, Precondition::None, None, |file| {
        file.rename(&"gitlab/edited".parse().unwrap(), "Edited")
    })
    .await
    .unwrap();

    assert_eq!(saved.snapshot.connections.len(), 1);
    assert!(diagnostics::entries().iter().any(|entry| {
        entry.scope == "config"
            && entry
                .message
                .contains("gitlab/edited: unknown setting `urll`")
    }));
}

/// A second export while one's dialog is open is refused as busy; once it ends, the next goes.
#[test]
fn one_export_at_a_time() {
    let turn = super::ExportTurn::default();
    let open = turn.take().unwrap();
    assert_eq!(
        turn.take().err().map(|err| err.kind),
        Some(BackendErrorKind::Busy)
    );
    drop(open);
    assert!(turn.take().is_ok());
}

/// B5: the file the user confirmed is written as named; a file with another extension that
/// the dialog never asked about stays as it was.
#[test]
fn an_export_is_written_exactly_where_the_user_chose() {
    let dir = tempfile::tempdir().unwrap();
    let other = dir.path().join("report.json");
    std::fs::write(&other, "keep me").unwrap();

    write_export(&dir.path().join("report.txt"), "export").unwrap();
    write_export(&dir.path().join("report"), "export").unwrap();

    assert_eq!(std::fs::read_to_string(&other).unwrap(), "keep me");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("report.txt")).unwrap(),
        "export"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("report")).unwrap(),
        "export"
    );
}

/// The pane gets the kind and the agreed text for a location that cannot keep the settings
/// private.
#[cfg(unix)]
#[tokio::test]
async fn an_unprotectable_location_is_reported_with_the_agreed_text() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let shared = dir.path().join("shared");
    std::fs::create_dir(&shared).unwrap();
    std::fs::set_permissions(&shared, std::fs::Permissions::from_mode(0o1777)).unwrap();
    let state = AppState::start(
        Some(shared.join("config.toml")),
        activity_log_core::secrets::Secrets::none(),
    );

    let refused = update_config(&state, Precondition::None, None, |file| {
        file.add(
            &activity_log_core::model::Source::Git,
            None,
            activity_log_core::provider::connection::ConnectionLocation::Local,
        )
    })
    .await
    .unwrap_err();

    assert_eq!(refused.kind, BackendErrorKind::Unprotectable);
    assert_eq!(
        refused.message,
        format!(
            "{} cannot keep the settings private; choose another location with ACTIVITY_LOG_CONFIG or --config",
            std::fs::canonicalize(&shared).unwrap().display()
        )
    );
}

/// B66: the save answers with the connection it added checking, however fast its check ends
/// on the other worker.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_save_answers_with_the_added_connection_checking() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let state = AppState::start(Some(path), activity_log_core::secrets::Secrets::none());

    for label in ["Team", "Home", "Club"] {
        let (created, saved) = update_config(&state, Precondition::None, None, move |file| {
            file.add(
                &activity_log_core::model::Source::Calendar,
                Some(label),
                activity_log_core::provider::connection::ConnectionLocation::Local,
            )
        })
        .await
        .unwrap();

        let added = saved
            .providers_response
            .providers
            .iter()
            .find(|info| info.connection == created)
            .unwrap();
        assert_eq!(
            added.status,
            activity_log_core::model::ProviderStatus::Checking
        );
    }
}

/// The handshake read is the pair a `state` event of the same value carries, byte for byte
/// once parsed.
#[test]
fn the_runtime_state_read_is_the_state_event_of_the_same_value() {
    let state = AppState::start(None, activity_log_core::secrets::Secrets::none());
    let published = state.host.subscribe().borrow().clone();
    let event = activity_log_core::runtime::RuntimeMessage::pair(&published);

    let read = invoke(state, "get_runtime_state", serde_json::json!({})).unwrap();

    assert_eq!(event.event(), "state");
    assert_eq!(
        read,
        serde_json::from_str::<serde_json::Value>(&event.to_json()).unwrap()
    );
}

/// The pane's check answers each issue at its field, without saving.
#[test]
fn a_settings_check_answers_the_issues_at_their_fields() {
    let text = "[[jira]]\nname = \"work\"\nlabel = \"Work\"\n";
    let (_dir, path, state) = settings_at(text);
    let request = serde_json::json!({
        "connection": { "source": "jira", "name": "work" },
        "values": [{ "key": "JIRA_URL", "value": "ftp://jira.example.org" }],
        "basedOn": snapshot_revision(&state),
    });

    let issues = invoke(
        state,
        "check_provider_config",
        serde_json::json!({ "request": request }),
    )
    .unwrap();

    assert_eq!(
        issues,
        serde_json::json!([{
            "field": { "kind": "value", "key": "JIRA_URL" },
            "problem": "invalid",
            "message": "Enter an http or https address.",
        }])
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
}

/// A settings file another program can change while the pane shows it.
fn settings_at(text: &str) -> (tempfile::TempDir, std::path::PathBuf, AppState) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, text).unwrap();
    let state = AppState::start(
        Some(path.clone()),
        activity_log_core::secrets::Secrets::none(),
    );
    (dir, path, state)
}

/// The revision of the snapshot the pane reads.
fn snapshot_revision(state: &AppState) -> String {
    let snapshot = invoke(state.clone(), "get_provider_config", serde_json::json!({})).unwrap();
    snapshot["revision"].as_str().unwrap().to_string()
}

/// Astra's first reproduction: the label changes on disk after the pane read the settings,
/// then Save. The save is refused as a conflict, the other program's label stays, and the
/// check names the field.
#[test]
fn a_save_over_a_field_another_program_changed_is_a_conflict() {
    let (_dir, path, state) = settings_at("[[gitlab]]\nlabel = \"Work\"\n");
    let revision = snapshot_revision(&state);
    std::fs::write(&path, "[[gitlab]]\nname = \"work\"\nlabel = \"Theirs\"\n").unwrap();
    let request = serde_json::json!({
        "connection": { "source": "gitlab", "name": "work" },
        "label": "Mine",
        "values": [],
        "basedOn": revision,
    });

    let issues = invoke(
        state.clone(),
        "check_provider_config",
        serde_json::json!({ "request": request }),
    )
    .unwrap();
    let refused = refusal(state.clone(), "save_provider_config", request);

    assert_eq!(refused.kind, BackendErrorKind::Conflict);
    // Named by the label the pane showed, as the agreed text says.
    assert!(
        refused
            .message
            .starts_with("Work was changed by another program"),
        "{}",
        refused.message
    );
    assert!(std::fs::read_to_string(&path).unwrap().contains("Theirs"));
    assert_eq!(issues[0]["field"], serde_json::json!({ "kind": "label" }));
    assert_eq!(issues[0]["problem"], "changed");
}

/// Astra's second reproduction: the table is replaced under the same id with another instance,
/// then Remove. The removal is refused, and the replacement stays.
#[test]
fn removing_a_connection_another_program_replaced_is_a_conflict() {
    let (_dir, path, state) = settings_at(
        "[[gitlab]]\nname = \"work\"\nlabel = \"Work\"\ninstance = \"0123456789abcdef\"\n",
    );
    let revision = snapshot_revision(&state);
    let replacement =
        "[[gitlab]]\nname = \"work\"\nlabel = \"Work\"\ninstance = \"fedcba9876543210\"\n";
    std::fs::write(&path, replacement).unwrap();

    let refused = refusal(
        state,
        "remove_connection",
        serde_json::json!({ "source": "gitlab", "name": "work", "basedOn": revision }),
    );

    assert_eq!(refused.kind, BackendErrorKind::Conflict);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), replacement);
}

/// A save based on the snapshot that is on disk goes through, and its answer names the next
/// revision; one based on a snapshot this app never handed out is a conflict.
#[test]
fn a_save_on_its_own_snapshot_applies_and_an_unknown_one_is_refused() {
    let (_dir, path, state) = settings_at("[[gitlab]]\nlabel = \"Work\"\n");
    let revision = snapshot_revision(&state);
    let save = |based_on: &str, label: &str| {
        serde_json::json!({ "request": {
            "connection": { "source": "gitlab", "name": "work" },
            "label": label,
            "values": [],
            "basedOn": based_on,
        }})
    };

    let saved = invoke(
        state.clone(),
        "save_provider_config",
        save(&revision, "Mine"),
    )
    .unwrap();
    assert!(std::fs::read_to_string(&path).unwrap().contains("Mine"));
    let next = saved["snapshot"]["revision"].as_str().unwrap();
    assert_ne!(next, revision);
    assert!(invoke(state.clone(), "save_provider_config", save(next, "Again")).is_ok());

    let refused = invoke(
        state,
        "save_provider_config",
        save("0000000000000000", "Lost"),
    )
    .unwrap_err();
    let refused: BackendError = serde_json::from_value(refused).unwrap();
    assert_eq!(refused.kind, BackendErrorKind::Conflict);
}

/// A save or a removal that names no snapshot is refused before its command runs (Tauri reads
/// the request), and nothing is written: there is no edit without a precondition.
#[test]
fn a_change_that_names_no_snapshot_is_refused_and_writes_nothing() {
    let text = "[[gitlab]]\nlabel = \"Work\"\n";
    let (_dir, path, state) = settings_at(text);
    let connection = serde_json::json!({ "source": "gitlab", "name": "work" });

    for (command, request) in [
        (
            "save_provider_config",
            serde_json::json!({ "connection": connection, "label": "Mine", "values": [] }),
        ),
        (
            "check_provider_config",
            serde_json::json!({ "connection": connection, "values": [] }),
        ),
        ("remove_connection", connection.clone()),
    ] {
        let refused = invoke(
            state.clone(),
            command,
            serde_json::json!({ "request": request }),
        )
        .unwrap_err();
        // Tauri's own refusal of the arguments, which the dashboard reads as `internal`.
        assert!(refused.is_string(), "{command}: {refused}");
    }
    assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
}

/// Astra's re-add probe, as the pane now sends it: another program restored `jira/default`, so
/// adding Jira again creates another connection, and the draft saved to the id the add answered
/// changes that one, never the restored one.
#[test]
fn a_draft_saved_into_a_source_added_again_changes_the_one_created() {
    let (_dir, path, state) = settings_at(
        "[[jira]]\nname = \"default\"\nlabel = \"Jira\"\ninstance = \"0123456789abcdef\"\n",
    );

    let added = invoke(
        state.clone(),
        "add_connection",
        serde_json::json!({ "request": { "source": "jira", "label": "Jira" } }),
    )
    .unwrap();
    let created = added["created"].clone();
    assert_ne!(
        created,
        serde_json::json!({ "source": "jira", "name": "default" })
    );

    invoke(
        state,
        "save_provider_config",
        serde_json::json!({ "request": {
            "connection": created,
            "label": "Mine",
            "values": [],
            "basedOn": added["snapshot"]["revision"],
        }}),
    )
    .unwrap();

    let labels = labels(&path);
    assert_eq!(labels, ["Jira", "Mine"]);
}

/// A token for a connection without an address is refused at its field by the check and as
/// `invalidRequest` by the save, with the OS store holding a key: it is never written as plain
/// text.
#[test]
fn a_token_without_an_address_is_refused() {
    use std::sync::Arc;

    use activity_log_core::secrets::{
        DataKey, FrontEnd, MemoryStore, SecretStore, Secrets, StoreKind,
    };

    let text = "[[gitlab]]\nname = \"new\"\nlabel = \"New\"\n";
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, text).unwrap();
    let os: Arc<dyn SecretStore> = Arc::new(MemoryStore::holding(
        StoreKind::Os,
        DataKey::generate().unwrap(),
    ));
    let file: Arc<dyn SecretStore> = Arc::new(MemoryStore::new(StoreKind::File));
    let state = AppState::start(
        Some(path.clone()),
        Secrets::with_stores(FrontEnd::Desktop, Some((os, None)), (file, None)),
    );
    let request = serde_json::json!({
        "connection": { "source": "gitlab", "name": "new" },
        "values": [{ "key": "GITLAB_TOKEN", "value": "glpat-test-0000" }],
        "basedOn": snapshot_revision(&state),
    });

    let issues = invoke(
        state.clone(),
        "check_provider_config",
        serde_json::json!({ "request": request }),
    )
    .unwrap();
    let refused = refusal(state, "save_provider_config", request);

    assert_eq!(
        issues[0]["field"],
        serde_json::json!({ "kind": "value", "key": "GITLAB_TOKEN" })
    );
    assert_eq!(issues[0]["problem"], "invalid");
    assert_eq!(refused.kind, BackendErrorKind::InvalidRequest);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
}
