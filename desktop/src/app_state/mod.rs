use std::{path::PathBuf, sync::Arc};

use activity_log_core::runtime::{BadConfig, Host, RunSpawner};

use crate::provider_config::SnapshotRegistry;

#[cfg(test)]
mod tests;

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) host: Host,
    /// The files of the settings snapshots handed out, for the bases of saves and removals.
    pub(crate) snapshots: Arc<SnapshotRegistry>,
}

impl AppState {
    /// The host of the settings file at `path`. A file that cannot be read or parsed is logged
    /// and the app starts without sources, so it opens and the file can be fixed; the file is
    /// watched either way once `watch_config` runs.
    pub(crate) fn start(path: Option<PathBuf>) -> Self {
        let host = Host::start(path, spawner(), BadConfig::StartEmpty)
            .expect("a host that starts empty on a bad file always starts");
        Self {
            host,
            snapshots: Arc::default(),
        }
    }
}

/// Runs host work on Tauri's runtime.
fn spawner() -> RunSpawner {
    Arc::new(|work| {
        tauri::async_runtime::spawn(work);
    })
}
