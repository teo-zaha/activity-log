use activity_log_core::runtime::CollectionRuntime;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) runtime: Arc<RwLock<CollectionRuntime>>,
    /// Held while `config.toml` is read, changed and saved, so settings commands that arrive
    /// together apply one after another instead of overwriting each other.
    pub(crate) config_edit: Arc<Mutex<()>>,
}

impl AppState {
    pub(crate) fn new(runtime: CollectionRuntime) -> Self {
        Self {
            runtime: Arc::new(RwLock::new(runtime)),
            config_edit: Arc::default(),
        }
    }
}
