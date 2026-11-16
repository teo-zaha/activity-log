use std::{future::Future, sync::Arc, time::Duration};

use futures::future::join_all;

use crate::{
    diagnostics,
    model::{
        ActivityEntry, CollectOptions, LogLevel, ProviderInfo, ProviderStatus, ProvidersResponse,
    },
    utils::now_iso,
};

use super::{
    current_os,
    provider_adapter::ProviderAdapter,
    provider_check_result::ProviderCheckResult,
    provider_config::{ConfigSource, EnvConfigSource},
    provider_context::ProviderContext,
    provider_registry,
};

// A hung OS integration (Outlook dialog, unreachable host, huge directory walk)
// must not block runtime rebuilds such as saving settings.
const STARTUP_CHECK_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone)]
pub struct ProviderRuntime {
    adapter: Arc<dyn ProviderAdapter>,
    context: ProviderContext,
    info: ProviderInfo,
}

impl ProviderRuntime {
    pub async fn inspect(
        adapter: Arc<dyn ProviderAdapter>,
        config_source: &dyn ConfigSource,
    ) -> Self {
        // Resolve provider config once so startup checks and later collections use the same input.
        let mut context = ProviderContext::resolve(config_source, adapter.config_requirements());
        let info = build_provider_info(adapter.as_ref(), &mut context).await;

        Self {
            adapter,
            context,
            info,
        }
    }

    pub fn id(&self) -> crate::model::Source {
        self.info.id.clone()
    }

    pub fn usable(&self) -> bool {
        self.info.usable
    }

    pub fn info(&self) -> &ProviderInfo {
        &self.info
    }

    pub fn info_owned(&self) -> ProviderInfo {
        self.info.clone()
    }

    pub async fn collect(&self, options: CollectOptions) -> anyhow::Result<Vec<ActivityEntry>> {
        let current_os = current_os();
        match self.adapter.cache_policy() {
            Some(policy) => {
                crate::cache::collect_with_day_cache(
                    &self.info.id,
                    self.context.cache_scope(),
                    &options,
                    policy,
                    |span| self.adapter.collect(current_os, span, self.context.clone()),
                )
                .await
            }
            None => {
                self.adapter
                    .collect(current_os, options, self.context.clone())
                    .await
            }
        }
    }
}

pub async fn inspect_provider_runtimes() -> Vec<ProviderRuntime> {
    inspect_provider_runtimes_with(&EnvConfigSource).await
}

pub async fn inspect_provider_runtimes_with(
    config_source: &dyn ConfigSource,
) -> Vec<ProviderRuntime> {
    join_all(
        provider_registry()
            .into_iter()
            .map(|adapter| ProviderRuntime::inspect(adapter, config_source)),
    )
    .await
}

pub fn providers_response_from_runtimes(runtimes: &[ProviderRuntime]) -> ProvidersResponse {
    ProvidersResponse {
        current_os: current_os(),
        providers: runtimes.iter().map(ProviderRuntime::info_owned).collect(),
    }
}

async fn build_provider_info(
    adapter: &dyn ProviderAdapter,
    context: &mut ProviderContext,
) -> ProviderInfo {
    let current_os = current_os();
    let requirements = adapter.config_requirements();
    let started = std::time::Instant::now();
    let result = if !adapter.supported_os().contains(&current_os) {
        ProviderCheckResult::unavailable("OS not supported")
    } else if !context.missing_required_keys(requirements).is_empty() {
        ProviderCheckResult::unavailable(format!("{} not configured", adapter.id()))
    } else if let Some(key) = context.first_invalid_key(requirements) {
        ProviderCheckResult::unavailable(format!("{} must be a {}", key.name, key.shape))
    } else if let Some(binding) = adapter.config_binding() {
        if let Err(err) = (binding.resolve)(context) {
            ProviderCheckResult::unavailable(format!("Config resolution failed: {err}"))
        } else {
            with_startup_timeout(
                adapter.startup_check(current_os.clone(), context.clone()),
                STARTUP_CHECK_TIMEOUT,
            )
            .await
        }
    } else {
        with_startup_timeout(
            adapter.startup_check(current_os.clone(), context.clone()),
            STARTUP_CHECK_TIMEOUT,
        )
        .await
    };

    let status = result.status;
    let elapsed = started.elapsed().as_millis();
    let summary = match &result.reason {
        Some(reason) => format!("Startup check {status} after {elapsed} ms: {reason}"),
        None => format!("Startup check {status} after {elapsed} ms"),
    };
    let level = match status {
        ProviderStatus::Available => LogLevel::Info,
        ProviderStatus::Degraded | ProviderStatus::Unavailable => LogLevel::Warn,
    };
    diagnostics::record(level, adapter.id().as_ref(), summary);

    ProviderInfo {
        id: adapter.id(),
        status: status.clone(),
        usable: status != ProviderStatus::Unavailable,
        supported_os: adapter.supported_os().to_vec(),
        reason: result.reason,
        checked_at: now_iso(),
    }
}

pub(crate) async fn with_startup_timeout(
    check: impl Future<Output = ProviderCheckResult>,
    timeout: Duration,
) -> ProviderCheckResult {
    tokio::time::timeout(timeout, check)
        .await
        .unwrap_or_else(|_| {
            ProviderCheckResult::unavailable(format!(
                "Startup check timed out after {}s",
                timeout.as_secs()
            ))
        })
}
