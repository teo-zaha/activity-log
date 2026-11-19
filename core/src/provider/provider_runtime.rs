use std::{future::Future, sync::Arc, time::Duration};

use futures::{
    FutureExt,
    future::{BoxFuture, join_all},
};

use crate::{
    diagnostics,
    model::{
        ActivityEntry, CollectOptions, ConnectionId, IdentityEvidence, LogLevel, ProviderInfo,
        ProviderProblem, ProviderStatus, ProvidersResponse,
    },
    utils::now_iso,
};

use super::{
    connection::{Connection, ConnectionLocation},
    current_os,
    provider_adapter::{CollectionState, ProviderAdapter},
    provider_check_result::ProviderCheckResult,
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
    identity: IdentityEvidence,
}

impl ProviderRuntime {
    pub async fn inspect(adapter: Arc<dyn ProviderAdapter>, connection: &Connection) -> Self {
        // Resolve provider config once so startup checks and later collections use the same input.
        let mut context =
            ProviderContext::for_connection(connection, adapter.config_requirements());
        let (info, identity) = diagnostics::in_connection(
            connection.id.clone(),
            build_provider_info(adapter.as_ref(), connection, &mut context),
        )
        .await;

        Self {
            adapter,
            context,
            info,
            identity,
        }
    }

    pub fn id(&self) -> &ConnectionId {
        &self.info.connection
    }

    pub fn adapter(&self) -> &Arc<dyn ProviderAdapter> {
        &self.adapter
    }

    pub fn usable(&self) -> bool {
        self.info.usable
    }

    pub fn info(&self) -> &ProviderInfo {
        &self.info
    }

    /// Whose token the startup check saw.
    pub fn identity(&self) -> &IdentityEvidence {
        &self.identity
    }

    pub fn info_owned(&self) -> ProviderInfo {
        self.info.clone()
    }

    /// Collects this connection; `state` is its kind's state for this collection.
    pub async fn collect(
        &self,
        options: CollectOptions,
        state: Option<CollectionState>,
    ) -> anyhow::Result<Vec<ActivityEntry>> {
        let current_os = current_os();
        let context = self.context.clone().with_collection_state(state);
        let collect = async {
            match self.adapter.cache_policy() {
                Some(policy) => {
                    crate::cache::collect_with_day_cache(
                        self.id(),
                        self.context.cache_scope(),
                        &options,
                        policy,
                        |span| self.adapter.collect(current_os, span, context.clone()),
                    )
                    .await
                }
                None => self.adapter.collect(current_os, options, context).await,
            }
        };
        diagnostics::in_connection(self.id().clone(), collect).await
    }
}

/// One runtime per connection, in connection order.
pub async fn inspect_provider_runtimes_with(connections: &[Connection]) -> Vec<ProviderRuntime> {
    join_all(
        start_checks(connections)
            .into_iter()
            .map(|(_, check)| check),
    )
    .await
}

/// For each connection of a known kind: what its info says while its startup check runs, and
/// that check.
pub fn start_checks(
    connections: &[Connection],
) -> Vec<(ProviderInfo, BoxFuture<'static, ProviderRuntime>)> {
    connections.iter().filter_map(start_check).collect()
}

/// `start_checks` for one connection; `None` for a kind no provider handles.
pub fn start_check(
    connection: &Connection,
) -> Option<(ProviderInfo, BoxFuture<'static, ProviderRuntime>)> {
    let adapter = provider_registry()
        .into_iter()
        .find(|adapter| adapter.id() == *connection.source())?;
    let checking = provider_info(
        adapter.as_ref(),
        connection,
        ProviderStatus::Checking,
        None,
        None,
    );
    let connection = connection.clone();
    let check = async move { ProviderRuntime::inspect(adapter, &connection).await };
    Some((checking, check.boxed()))
}

pub fn providers_response_from_runtimes(runtimes: &[ProviderRuntime]) -> ProvidersResponse {
    ProvidersResponse {
        current_os: current_os(),
        providers: runtimes.iter().map(ProviderRuntime::info_owned).collect(),
    }
}

async fn build_provider_info(
    adapter: &dyn ProviderAdapter,
    connection: &Connection,
    context: &mut ProviderContext,
) -> (ProviderInfo, IdentityEvidence) {
    let current_os = current_os();
    let requirements = adapter.config_requirements();
    let started = std::time::Instant::now();
    let result = if !adapter.supported_os().contains(&current_os) {
        unavailable(ProviderProblem::Unsupported, "OS not supported")
    } else if !context.missing_required_keys(requirements).is_empty() {
        unavailable(
            ProviderProblem::NotConfigured,
            format!("{} is not configured", connection.label),
        )
    } else if let Some(key) = context.first_invalid_key(requirements) {
        unavailable(
            ProviderProblem::NotConfigured,
            format!("{} must be a {}", key.name, key.shape),
        )
    } else if let Some(binding) = adapter.config_binding() {
        if let Err(err) = (binding.resolve)(context) {
            unavailable(
                ProviderProblem::NotConfigured,
                format!("Config resolution failed: {err}"),
            )
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
        ProviderStatus::Checking | ProviderStatus::Available => LogLevel::Info,
        ProviderStatus::Degraded | ProviderStatus::Unavailable => LogLevel::Warn,
    };
    diagnostics::record(level, connection.id.to_string(), summary);

    (
        provider_info(adapter, connection, status, result.problem, result.reason),
        result.identity,
    )
}

fn unavailable(problem: ProviderProblem, reason: impl Into<String>) -> ProviderCheckResult {
    ProviderCheckResult::problem(ProviderStatus::Unavailable, problem, reason)
}

fn provider_info(
    adapter: &dyn ProviderAdapter,
    connection: &Connection,
    status: ProviderStatus,
    problem: Option<ProviderProblem>,
    reason: Option<String>,
) -> ProviderInfo {
    ProviderInfo {
        connection: connection.id.clone(),
        label: connection.label.clone(),
        wsl_distro: match &connection.location {
            ConnectionLocation::Wsl(distro) => Some(distro.clone()),
            ConnectionLocation::Local => None,
        },
        usable: matches!(status, ProviderStatus::Available | ProviderStatus::Degraded),
        status,
        supported_os: adapter.supported_os().to_vec(),
        reason,
        problem,
        checked_at: now_iso(),
    }
}

tokio::task_local! {
    /// When the running startup check is cut off.
    static CHECK_DEADLINE: tokio::time::Instant;
}

/// Time left before the running startup check is cut off; `None` outside a check.
pub(crate) fn check_time_left() -> Option<Duration> {
    CHECK_DEADLINE
        .try_with(|deadline| deadline.saturating_duration_since(tokio::time::Instant::now()))
        .ok()
}

pub(crate) async fn with_startup_timeout(
    check: impl Future<Output = ProviderCheckResult>,
    timeout: Duration,
) -> ProviderCheckResult {
    let deadline = tokio::time::Instant::now() + timeout;
    CHECK_DEADLINE
        .scope(deadline, tokio::time::timeout_at(deadline, check))
        .await
        .unwrap_or_else(|_| {
            unavailable(
                ProviderProblem::TimedOut,
                format!("Startup check timed out after {}s", timeout.as_secs()),
            )
        })
}
