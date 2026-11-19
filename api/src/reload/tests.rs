use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use activity_log_core::{
    model::CollectRequest,
    runtime::{ActivitySnapshot, CollectionRuntime, Collector, resolve_collect_options},
};

use super::ApiRuntime;

fn empty_runtime() -> CollectionRuntime {
    let collector: Collector = Arc::new(|_, _, _, _| Box::pin(std::future::pending()));
    CollectionRuntime::new(collector, Vec::new())
}

#[tokio::test]
async fn rebuilds_only_when_the_config_file_changes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "gitlab = []\n").unwrap();
    let builds = Arc::new(AtomicUsize::new(0));
    let counter = builds.clone();
    let api = ApiRuntime::with_builder(
        path.clone(),
        Arc::new(move |_| {
            counter.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Ok(empty_runtime()) })
        }),
    )
    .await
    .unwrap();

    api.runtime().await;
    assert_eq!(builds.load(Ordering::SeqCst), 1);

    touch(&path);
    api.runtime().await;
    api.runtime().await;
    assert_eq!(builds.load(Ordering::SeqCst), 2);
}

/// Marks the config file as changed. Some file systems keep modification times in whole
/// seconds, so the new time is well apart.
fn touch(path: &std::path::Path) {
    let later = std::time::SystemTime::now() + std::time::Duration::from_secs(5);
    let file = std::fs::File::options().write(true).open(path).unwrap();
    file.set_modified(later).unwrap();
}

/// The builder hands out a fresh runtime each time, so one that is running after a reload
/// took the collection over.
#[tokio::test]
async fn a_reload_keeps_the_collection_running() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "gitlab = []\n").unwrap();
    let api = ApiRuntime::with_builder(
        path.clone(),
        Arc::new(|_| Box::pin(async { Ok(empty_runtime()) })),
    )
    .await
    .unwrap();
    let options = resolve_collect_options(&CollectRequest {
        start: "2026-03-01T00:00:00Z".to_string(),
        end: "2026-03-06T00:00:00Z".to_string(),
    })
    .unwrap();
    api.start_collection(options).await;

    touch(&path);
    let reloaded = api.runtime().await;

    assert!(matches!(
        reloaded.activity_snapshot(),
        ActivitySnapshot::Running(log) if log.meta.start == options.start.to_rfc3339()
    ));
}
