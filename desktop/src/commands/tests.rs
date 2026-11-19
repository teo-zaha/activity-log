use std::{sync::mpsc, time::Duration};

use activity_log_core::{
    diagnostics,
    settings::{ConfigFile, NO_LOCATION},
};

use super::{update_config, web_url, write_export};
use crate::app_state::AppState;

fn labels(path: &std::path::Path) -> Vec<String> {
    let file = ConfigFile::load(path).unwrap();
    let connections = file.connections().unwrap().connections;
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
    let ids = ConfigFile::load(&path).unwrap().connections().unwrap();
    let [a, b] = <[_; 2]>::try_from(ids.connections)
        .ok()
        .unwrap()
        .map(|c| c.id);
    let state = AppState::start(Some(path.clone()));
    let (first_loaded, first_has_loaded) = mpsc::channel();
    let (second_loaded, second_has_loaded) = mpsc::channel();

    let first = tokio::spawn({
        let state = state.clone();
        async move {
            update_config(&state, move |file| {
                first_loaded.send(()).unwrap();
                // Gives the second command every chance to read the file before this one saves.
                let _ = second_has_loaded.recv_timeout(Duration::from_millis(300));
                file.rename(&a, "A changed").map_err(|err| err.to_string())
            })
            .await
        }
    });
    first_has_loaded.recv().unwrap();
    update_config(&state, move |file| {
        let _ = second_loaded.send(());
        file.rename(&b, "B changed").map_err(|err| err.to_string())
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
        assert!(web_url(link).is_err(), "{link}");
    }
}

/// Without a settings location a save fails with the message, and nothing is written.
#[tokio::test]
async fn a_save_without_a_settings_location_writes_nothing() {
    let state = AppState::start(None);

    let refused = update_config(&state, |file| {
        file.rename(&"gitlab/work".parse().unwrap(), "Work")
            .map_err(|err| err.to_string())
    })
    .await;

    assert_eq!(refused.err().as_deref(), Some(NO_LOCATION));
}

#[tokio::test]
async fn a_busy_settings_file_is_reported_and_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "[[git]]\n").unwrap();
    let held = std::fs::File::create(dir.path().join("config.toml.lock")).unwrap();
    held.lock().unwrap();
    let state = AppState::start(Some(path.clone()));

    let refused = update_config(&state, |file| {
        file.rename(&"git/this-computer".parse().unwrap(), "Laptop")
            .map_err(|err| err.to_string())
    })
    .await;

    assert_eq!(
        refused.err().as_deref(),
        Some("The settings file is busy; try again.")
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "[[git]]\n");
    drop(held);
}

#[tokio::test]
async fn config_problems_after_an_edit_reach_the_logs() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "[[gitlab]]\nlabel = \"Edited\"\nurll = \"x\"\n").unwrap();
    let state = AppState::start(Some(path));

    let (_, saved) = update_config(&state, |file| {
        file.rename(&"gitlab/edited".parse().unwrap(), "Edited")
            .map_err(|err| err.to_string())
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

/// The pane gets the agreed text for a location that cannot keep the settings private.
#[cfg(unix)]
#[tokio::test]
async fn an_unprotectable_location_is_reported_with_the_agreed_text() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let shared = dir.path().join("shared");
    std::fs::create_dir(&shared).unwrap();
    std::fs::set_permissions(&shared, std::fs::Permissions::from_mode(0o1777)).unwrap();
    let state = AppState::start(Some(shared.join("config.toml")));

    let refused = update_config(&state, |file| {
        file.add(
            &activity_log_core::model::Source::Git,
            None,
            activity_log_core::provider::connection::ConnectionLocation::Local,
        )
        .map_err(|err| err.to_string())
    })
    .await
    .unwrap_err();

    assert_eq!(
        refused,
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
    let state = AppState::start(Some(path));

    for label in ["Team", "Home", "Club"] {
        let (created, saved) = update_config(&state, move |file| {
            file.add(
                &activity_log_core::model::Source::Calendar,
                Some(label),
                activity_log_core::provider::connection::ConnectionLocation::Local,
            )
            .map_err(|err| err.to_string())
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
