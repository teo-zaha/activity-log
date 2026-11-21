use std::{future::Future, path::Path, sync::Arc, time::Duration};

use futures::{
    FutureExt,
    future::{BoxFuture, join_all},
};

use crate::{
    diagnostics,
    model::{
        ActivityEntry, CollectOptions, ConnectionId, ConnectionStamp, IdentityEvidence, LogLevel,
        ProviderInfo, ProviderProblem, ProviderStatus, Source, SupportedOs,
    },
    utils::now_iso,
};

use crate::cache::Uncached;

use super::{
    adapter,
    connection::{Connection, ConnectionLocation},
    current_os,
    provider_adapter::{CollectionState, ProviderAdapter},
    provider_check_result::ProviderCheckResult,
    provider_context::ProviderContext,
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

    /// A runtime that only reports `info`, for a host built from a fixed list of checked
    /// providers: it is never checked again and collects nothing.
    pub fn from_info(info: ProviderInfo) -> Self {
        let connection = Connection::new(
            info.connection.clone(),
            info.label.clone(),
            ConnectionLocation::Local,
            Default::default(),
        );
        Self {
            adapter: Arc::new(Recorded(info.connection.source.clone())),
            context: ProviderContext::for_connection(&connection, &[]),
            info,
            identity: IdentityEvidence::None,
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

    /// The day cache's scope for this connection: its safe fingerprint and the principal the
    /// check verified. Two tokens of one account share it; without a verified identity, or with
    /// a URL holding credentials, the connection is not cached.
    pub(crate) fn cache_scope(&self) -> Result<u64, Uncached> {
        let fingerprint = self
            .context
            .fingerprint(self.adapter.config_requirements())
            .map_err(Uncached::Url)?;
        match &self.identity {
            IdentityEvidence::Verified(identity) => Ok(crate::utils::fnv1a(
                format!("{fingerprint:016x}\n{}", identity.id).into_bytes(),
            )),
            _ => Err(Uncached::Unverified),
        }
    }

    /// Collects this connection; `state` is its kind's state for this collection.
    pub async fn collect(
        &self,
        options: CollectOptions,
        state: Option<CollectionState>,
    ) -> anyhow::Result<Vec<ActivityEntry>> {
        self.collect_in(&crate::cache::cache_dir(), options, state)
            .await
    }

    /// [`Self::collect`] with the day cache in `cache_dir`.
    pub(crate) async fn collect_in(
        &self,
        cache_dir: &Path,
        options: CollectOptions,
        state: Option<CollectionState>,
    ) -> anyhow::Result<Vec<ActivityEntry>> {
        let context = self.context.clone().with_collection_state(state);
        // Collectors know no connection: their entries are stamped here, before the day cache
        // stores them, so none is stored or sent without one. A connection's name is a slug of
        // its label, never empty.
        let name = &ConnectionStamp::new(&self.id().name).ok_or_else(|| {
            anyhow::anyhow!("{} has no name to stamp its entries with", self.id())
        })?;
        let fetch = |span| {
            let fetched = self.adapter.collect(span, context.clone());
            async move {
                let mut entries = fetched.await?;
                for entry in &mut entries {
                    entry.set_connection(name);
                }
                Ok(entries)
            }
        };
        let collect = async {
            let Some(policy) = self.adapter.cache_policy() else {
                return fetch(options).await;
            };
            match self.cache_scope() {
                Ok(scope) => {
                    crate::cache::collect_with_day_cache(
                        cache_dir,
                        self.id(),
                        scope,
                        &options,
                        policy,
                        fetch,
                    )
                    .await
                }
                Err(uncached) => {
                    uncached.log(&self.id().to_string());
                    fetch(options).await
                }
            }
        };
        diagnostics::in_connection(self.id().clone(), collect).await
    }
}

/// The adapter of [`ProviderRuntime::from_info`].
struct Recorded(Source);

impl ProviderAdapter for Recorded {
    fn id(&self) -> Source {
        self.0.clone()
    }

    fn supported_os(&self) -> &'static [SupportedOs] {
        &[]
    }

    fn description(&self) -> &'static str {
        ""
    }

    fn startup_check(&self, _context: ProviderContext) -> BoxFuture<'_, ProviderCheckResult> {
        Box::pin(async { ProviderCheckResult::available() })
    }

    fn collect(
        &self,
        _options: CollectOptions,
        _context: ProviderContext,
    ) -> BoxFuture<'_, anyhow::Result<Vec<ActivityEntry>>> {
        Box::pin(async { Ok(Vec::new()) })
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

/// For each connection: what its info says while its startup check runs, and that check.
pub fn start_checks(
    connections: &[Connection],
) -> Vec<(ProviderInfo, BoxFuture<'static, ProviderRuntime>)> {
    connections.iter().map(start_check).collect()
}

/// `start_checks` for one connection.
pub fn start_check(connection: &Connection) -> (ProviderInfo, BoxFuture<'static, ProviderRuntime>) {
    start_check_with(adapter(connection.source()).clone(), connection)
}

/// `start_check` with the adapter given.
pub(crate) fn start_check_with(
    adapter: Arc<dyn ProviderAdapter>,
    connection: &Connection,
) -> (ProviderInfo, BoxFuture<'static, ProviderRuntime>) {
    let checking = provider_info(adapter.as_ref(), connection);
    let connection = connection.clone();
    let check = async move { ProviderRuntime::inspect(adapter, &connection).await };
    (checking, check.boxed())
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
                adapter.startup_check(context.clone()),
                STARTUP_CHECK_TIMEOUT,
            )
            .await
        }
    } else {
        with_startup_timeout(
            adapter.startup_check(context.clone()),
            STARTUP_CHECK_TIMEOUT,
        )
        .await
    };

    let status = result.outcome.status();
    let elapsed = started.elapsed().as_millis();
    let summary = match result.outcome.reason() {
        Some(reason) => format!("Startup check {status} after {elapsed} ms: {reason}"),
        None => format!("Startup check {status} after {elapsed} ms"),
    };
    let level = match status {
        ProviderStatus::Checking | ProviderStatus::Available => LogLevel::Info,
        ProviderStatus::Degraded | ProviderStatus::Unavailable => LogLevel::Warn,
    };
    diagnostics::record(level, connection.id.to_string(), summary);

    let mut info = provider_info(adapter, connection);
    result.outcome.apply_to(&mut info);
    (info, result.identity)
}

fn unavailable(problem: ProviderProblem, reason: impl Into<String>) -> ProviderCheckResult {
    ProviderCheckResult::unavailable_as(problem, reason)
}

/// `connection` as it shows while its check runs: not usable, no problem yet.
fn provider_info(adapter: &dyn ProviderAdapter, connection: &Connection) -> ProviderInfo {
    ProviderInfo {
        connection: connection.id.clone(),
        label: connection.label.clone(),
        wsl_distro: match &connection.location {
            ConnectionLocation::Wsl(distro) => Some(distro.clone()),
            ConnectionLocation::Local => None,
        },
        status: ProviderStatus::Checking,
        usable: false,
        supported_os: adapter.supported_os().to_vec(),
        reason: None,
        problem: None,
        recheckable: false,
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
