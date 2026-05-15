use activity_log_core::runtime::CollectionRuntime;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) runtime: Arc<RwLock<CollectionRuntime>>,
}
