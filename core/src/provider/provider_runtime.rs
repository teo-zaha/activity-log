use std::sync::Arc;

use futures::future::join_all;

use crate::{
    model::{CollectOptions, ProviderInfo, ProviderStatus, ProvidersResponse},
    utils::now_iso,
};

use super::{
    current_os,
    provider_adapter::ProviderAdapter,
    provider_check_result::ProviderCheckResult,
    provider_collection::ProviderCollection,
    provider_config::{ConfigSource, EnvConfigSource},
    provider_context::ProviderContext,
    provider_registry,
};

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
        let context = ProviderContext::resolve(config_source, adapter.config_requirements());
        let info = build_provider_info(adapter.as_ref(), &context).await;

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

    pub async fn collect(&self, options: CollectOptions) -> anyhow::Result<ProviderCollection> {
        let current_os = current_os();
        self.adapter
            .collect(current_os, options, self.context.clone())
            .await
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
    context: &ProviderContext,
) -> ProviderInfo {
    let current_os = current_os();
    let result = if !adapter.supported_os().contains(&current_os) {
        ProviderCheckResult::unavailable("OS not supported")
    } else if !context
        .missing_required_keys(adapter.config_requirements())
        .is_empty()
    {
        ProviderCheckResult::unavailable(format!("{} not configured", adapter.id()))
    } else if let Some(key) = context.first_invalid_key(adapter.config_requirements()) {
        ProviderCheckResult::unavailable(format!("{} must be a {}", key.name, key.value_type))
    } else {
        adapter
            .startup_check(current_os.clone(), context.clone())
            .await
    };

    let status = result.status;
    ProviderInfo {
        id: adapter.id(),
        status: status.clone(),
        usable: status != ProviderStatus::Unavailable,
        supported_os: adapter.supported_os().to_vec(),
        reason: result.reason,
        checked_at: now_iso(),
    }
}
