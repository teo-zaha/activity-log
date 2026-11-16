use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use anyhow::Result as AnyhowResult;
use chrono::{DateTime, Duration, Utc};
use tokio::sync::RwLock;

use crate::{
    collect_activity_with_provider_runtimes,
    model::{ActivityLog, CollectOptions, CollectRequest, ProviderInfo, ProvidersResponse},
    provider::{
        ProviderRuntime, current_os, inspect_provider_runtimes_with,
        provider_config::{ConfigSource, EnvConfigSource},
    },
};

#[cfg(test)]
mod tests;

pub type CollectorFuture =
    Pin<Box<dyn Future<Output = AnyhowResult<ActivityLog>> + Send + 'static>>;
pub type Collector = Arc<dyn Fn(CollectOptions) -> CollectorFuture + Send + Sync>;
pub type RuntimeTask = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

#[derive(Clone)]
pub struct CollectionRuntime {
    inner: Arc<CollectionRuntimeInner>,
}

struct CollectionRuntimeInner {
    cached_log: RwLock<Option<ActivityLog>>,
    provider_info: Vec<ProviderInfo>,
    is_collecting: AtomicBool,
    collector: Collector,
}

pub enum ActivitySnapshot {
    Collecting,
    Ready(ActivityLog),
    Empty,
}

pub enum CollectionResult {
    Collecting,
    Ready(ActivityLog),
}

impl CollectionRuntime {
    pub fn new(collector: Collector, provider_info: Vec<ProviderInfo>) -> Self {
        Self {
            inner: Arc::new(CollectionRuntimeInner {
                cached_log: RwLock::new(None),
                provider_info,
                is_collecting: AtomicBool::new(false),
                collector,
            }),
        }
    }

    pub async fn build_default() -> AnyhowResult<Self> {
        Self::build_with_config_source(&EnvConfigSource).await
    }

    pub async fn build_with_config_source(config_source: &dyn ConfigSource) -> AnyhowResult<Self> {
        let provider_runtimes = inspect_provider_runtimes_with(config_source).await;
        let provider_info = provider_runtimes
            .iter()
            .map(ProviderRuntime::info_owned)
            .collect();
        Ok(Self::new(
            default_collector(provider_runtimes),
            provider_info,
        ))
    }

    pub async fn cached_log(&self) -> Option<ActivityLog> {
        self.inner.cached_log.read().await.clone()
    }

    pub fn provider_info(&self) -> Vec<ProviderInfo> {
        self.inner.provider_info.clone()
    }

    pub fn providers_response(&self) -> ProvidersResponse {
        ProvidersResponse {
            current_os: current_os(),
            providers: self.provider_info(),
        }
    }

    pub fn is_collecting(&self) -> bool {
        self.inner.is_collecting.load(Ordering::SeqCst)
    }

    pub async fn activity_snapshot(&self) -> ActivitySnapshot {
        if self.is_collecting() {
            return ActivitySnapshot::Collecting;
        }

        match self.cached_log().await {
            Some(log) => ActivitySnapshot::Ready(log),
            None => ActivitySnapshot::Empty,
        }
    }

    pub async fn collect_with_options(
        &self,
        options: CollectOptions,
    ) -> AnyhowResult<CollectionResult> {
        if self
            .inner
            .is_collecting
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Ok(CollectionResult::Collecting);
        }

        let log = self.run_locked_collection(options).await?;
        Ok(CollectionResult::Ready(log))
    }

    pub fn initial_collection_task(&self) -> Option<RuntimeTask> {
        if self
            .inner
            .is_collecting
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return None;
        }

        let runtime = self.clone();
        Some(Box::pin(async move {
            if let Err(err) = runtime
                .run_locked_collection(default_collect_options())
                .await
            {
                crate::diagnostics::error("runtime", format!("Initial collection failed: {err}"));
            }
        }))
    }

    async fn store(&self, log: ActivityLog) {
        *self.inner.cached_log.write().await = Some(log);
    }

    async fn run_locked_collection(&self, options: CollectOptions) -> AnyhowResult<ActivityLog> {
        let result = (self.inner.collector)(options).await;
        self.inner.is_collecting.store(false, Ordering::SeqCst);

        match result {
            Ok(log) => {
                self.store(log.clone()).await;
                Ok(log)
            }
            Err(err) => Err(err),
        }
    }
}

pub fn resolve_collect_options(
    request: &CollectRequest,
) -> std::result::Result<CollectOptions, &'static str> {
    let parse = |value: &str| {
        DateTime::parse_from_rfc3339(value)
            .map(|instant| instant.with_timezone(&Utc))
            .map_err(|_| "start and end must be RFC 3339 timestamps")
    };
    let options = CollectOptions {
        start: parse(&request.start)?,
        end: parse(&request.end)?,
    };
    if options.end <= options.start {
        return Err("end must be after start");
    }
    Ok(options)
}

fn default_collector(provider_runtimes: Vec<ProviderRuntime>) -> Collector {
    let provider_runtimes = Arc::new(provider_runtimes);
    Arc::new(move |options| {
        let provider_runtimes = Arc::clone(&provider_runtimes);
        Box::pin(async move {
            collect_activity_with_provider_runtimes(options, provider_runtimes.as_ref()).await
        })
    })
}

/// The last 30 days up to now; the dashboard re-collects whole local days when asked.
fn default_collect_options() -> CollectOptions {
    let end = Utc::now();
    CollectOptions {
        start: end - Duration::days(30),
        end,
    }
}
