use std::{sync::mpsc, time::Duration};

use activity_log_core::{runtime::CollectionRuntime, settings::ConfigFile};

use super::update_config_at;
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
    let state = AppState::new(CollectionRuntime::start(&[]).0);
    let (first_loaded, first_has_loaded) = mpsc::channel();
    let (second_loaded, second_has_loaded) = mpsc::channel();

    let first = tokio::spawn({
        let (state, path) = (state.clone(), path.clone());
        async move {
            update_config_at(&state, path, move |file| {
                first_loaded.send(()).unwrap();
                // Gives the second command every chance to read the file before this one saves.
                let _ = second_has_loaded.recv_timeout(Duration::from_millis(300));
                file.rename(&a, "A changed").map_err(|err| err.to_string())
            })
            .await
        }
    });
    first_has_loaded.recv().unwrap();
    update_config_at(&state, path.clone(), |file| {
        let _ = second_loaded.send(());
        file.rename(&b, "B changed").map_err(|err| err.to_string())
    })
    .await
    .unwrap();
    first.await.unwrap().unwrap();

    assert_eq!(labels(&path), ["A changed", "B changed"]);
}
