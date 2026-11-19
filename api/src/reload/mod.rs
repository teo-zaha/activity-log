//! The API server's runtime, rebuilt when `config.toml` changes, so edits from the CLI or the
//! desktop app apply without restarting the server.

use std::{
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
    time::SystemTime,
};

use activity_log_core::{
    diagnostics,
    model::{CollectOptions, ConnectionId, ProvidersResponse},
    runtime::{ActivitySnapshot, CollectionRuntime},
    settings::load_connections,
};
use tokio::sync::{Mutex, MutexGuard};

#[cfg(test)]
mod tests;

type BuildFuture = Pin<Box<dyn Future<Output = anyhow::Result<CollectionRuntime>> + Send>>;
type Build = Arc<dyn Fn(PathBuf) -> BuildFuture + Send + Sync>;

pub struct ApiRuntime {
    config: Option<(PathBuf, Build)>,
    current: Mutex<Loaded>,
}

struct Loaded {
    runtime: CollectionRuntime,
    modified: Option<SystemTime>,
}

impl ApiRuntime {
    /// Reads the connections from `path` and checks them.
    pub async fn from_config(path: PathBuf) -> anyhow::Result<Self> {
        Self::with_builder(path, Arc::new(|path| Box::pin(build_from(path)))).await
    }

    pub(crate) async fn with_builder(path: PathBuf, build: Build) -> anyhow::Result<Self> {
        let modified = modified(&path);
        let runtime = build(path.clone()).await?;
        Ok(Self {
            config: Some((path, build)),
            current: Mutex::new(Loaded { runtime, modified }),
        })
    }

    /// A runtime that never reloads, for tests.
    pub fn fixed(runtime: CollectionRuntime) -> Self {
        Self {
            config: None,
            current: Mutex::new(Loaded {
                runtime,
                modified: None,
            }),
        }
    }

    /// The current runtime, see [`Self::current`].
    pub async fn runtime(&self) -> CollectionRuntime {
        self.current().await.runtime.clone()
    }

    /// Starts collecting `options` on the current runtime, or reports the collection that
    /// already runs. Holds the lock, so a reload cannot swap the runtime in between.
    pub async fn start_collection(&self, options: CollectOptions) -> ActivitySnapshot {
        let current = self.current().await;
        let (snapshot, task) = current.runtime.start_collection(options);
        if let Some(task) = task {
            tokio::spawn(task);
        }
        snapshot
    }

    /// Runs the startup check of `id` again on the current runtime, which collects its kind
    /// again; `None` for a connection it does not know.
    pub async fn recheck(&self, id: &ConnectionId) -> Option<ProvidersResponse> {
        let current = self.current().await;
        let task = current.runtime.recheck(id)?;
        // Before the task runs, so the answer shows the connection checking however fast it is.
        let response = current.runtime.providers_response();
        tokio::spawn(task);
        Some(response)
    }

    pub async fn start_initial_collection(&self) {
        let current = self.current().await;
        if let Some(task) = current.runtime.initial_collection_task() {
            tokio::spawn(task);
        }
    }

    /// The current runtime, rebuilt first if the config file changed since it was read; the
    /// new one takes over the collection the old one was showing. A rebuild that fails is
    /// logged and the previous runtime stays.
    async fn current(&self) -> MutexGuard<'_, Loaded> {
        let mut current = self.current.lock().await;
        if let Some((path, build)) = &self.config {
            let modified = modified(path);
            if modified != current.modified {
                current.modified = modified;
                match build(path.clone()).await {
                    Ok(runtime) => {
                        diagnostics::info("config", format!("Reloaded {}", path.display()));
                        if let Some(task) = runtime.take_over(&current.runtime) {
                            tokio::spawn(task);
                        }
                        current.runtime = runtime;
                    }
                    Err(err) => diagnostics::error(
                        "config",
                        format!("Could not reload {}: {err:#}", path.display()),
                    ),
                }
            }
        }
        current
    }
}

async fn build_from(path: PathBuf) -> anyhow::Result<CollectionRuntime> {
    let (runtime, checks) = CollectionRuntime::start(&load_connections(Some(&path))?);
    tokio::spawn(checks);
    Ok(runtime)
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
}
