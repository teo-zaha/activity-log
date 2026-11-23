use std::{path::PathBuf, sync::Arc};

use activity_log_core::{
    model::RunEnd,
    runtime::{BadConfig, Host, RunSpawner},
    secrets::Secrets,
};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

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
    /// watched either way once `watch_config` runs. Nobody follows its runs' ends.
    #[cfg(test)]
    pub(crate) fn start(path: Option<PathBuf>, secrets: Secrets) -> Self {
        Self::start_following(path, secrets).0
    }

    /// `start`, and how each collection run ends, for `collection_notice::follow`. `secrets`
    /// opens the connections' sealed secrets; only `lib.rs` builds one over the user's stores.
    pub(crate) fn start_following(
        path: Option<PathBuf>,
        secrets: Secrets,
    ) -> (Self, UnboundedReceiver<RunEnd>) {
        let (ends, ended) = unbounded_channel();
        let host = Host::start(path, spawner(ends), BadConfig::StartEmpty, secrets)
            .expect("a host that starts empty on a bad file always starts");
        let state = Self {
            host,
            snapshots: Arc::default(),
        };
        (state, ended)
    }
}

/// Runs host work on Tauri's runtime and sends each run's end to `ends`. It holds nothing that
/// owns the host: the decision what to show is `collection_notice`'s.
fn spawner(ends: UnboundedSender<RunEnd>) -> RunSpawner {
    Arc::new(move |work| {
        let ends = ends.clone();
        tauri::async_runtime::spawn(async move {
            if let Some(end) = work.await {
                // Nobody follows the ends once the receiver is gone (tests, `start`).
                let _ = ends.send(end);
            }
        });
    })
}
