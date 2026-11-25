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
    secrets::{
        Secrets,
        opening::{Unopened, open_connection},
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
    source_failure::{ClassifiedProblem, FixCause},
};

// A hung OS integration (Outlook dialog, unreachable host, huge directory walk)
// must not block runtime rebuilds such as saving settings.
const STARTUP_CHECK_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone)]
pub struct ProviderRuntime {
    adapter: Arc<dyn ProviderAdapter>,
    context: ProviderContext,
    info: ProviderInfo,
    fix_cause: Option<FixCause>,
}

impl ProviderRuntime {
    /// Checks `connection`: its sealed secrets are opened first, with `secrets`, before the
    /// check's own timeout starts (a keychain dialog is no slow server), and the config is
    /// resolved once, so the check and every later collection read the same values; no
    /// collection asks a store again.
    pub async fn inspect(
        adapter: Arc<dyn ProviderAdapter>,
        connection: &Connection,
        secrets: &Secrets,
    ) -> Self {
        let requirements = adapter.config_requirements();
        let supported = adapter.supported_os().contains(&current_os());
        let (result, context) = diagnostics::in_connection(connection.id.clone(), async {
            let opened = if supported {
                open_connection(connection.source(), connection.values(), secrets).await
            } else {
                Ok(Default::default())
            };
            let (mut context, unopened) = match &opened {
                Ok(opened) => (
                    ProviderContext::for_connection_with(
                        connection,
                        &opened.over(connection.values()),
                        requirements,
                    ),
                    None,
                ),
                Err(unopened) => (
                    ProviderContext::for_connection(connection, requirements),
                    Some(unopened),
                ),
            };
            context.set_state_dir(adapter.cache_policy().map(|_| crate::cache::cache_dir()));
            let result = check_provider(adapter.as_ref(), connection, &mut context, unopened).await;
            (result, context)
        })
        .await;

        let info = provider_info(adapter.as_ref(), connection);
        let mut runtime = Self {
            adapter,
            context,
            info,
            fix_cause: None,
        };
        runtime.record_check_result(result);
        runtime
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
            fix_cause: None,
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

    /// Recovery evidence from this connection's startup check.
    pub fn fix_cause(&self) -> Option<FixCause> {
        self.fix_cause
    }

    /// Whose token the startup check saw.
    pub fn identity(&self) -> &IdentityEvidence {
        self.context.identity()
    }

    pub(crate) fn record_check_result(&mut self, result: ProviderCheckResult) {
        result.outcome.apply_to(&mut self.info);
        self.info.checked_at = now_iso();
        self.fix_cause = result
            .outcome
            .classification()
            .and_then(ClassifiedProblem::cause);
        self.context.set_identity(result.identity);
    }

    /// The day cache's scope for this connection: its safe fingerprint and the principal the
    /// check verified. Two tokens of one account share it; without a verified identity, or with
    /// a URL holding credentials, the connection is not cached.
    pub(crate) fn cache_scope(&self) -> Result<u64, Uncached> {
        let fingerprint = self
            .context
            .fingerprint(self.adapter.config_requirements())
            .map_err(Uncached::Url)?;
        match self.identity() {
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
        // Collectors know no connection: their entries are stamped here, before the day cache
        // stores them, so none is stored or sent without one. A connection's name is a slug of
        // its label, never empty.
        let name = &ConnectionStamp::new(&self.id().name).ok_or_else(|| {
            anyhow::anyhow!("{} has no name to stamp its entries with", self.id())
        })?;
        let collect = async {
            let cache = self
                .adapter
                .cache_policy()
                .and_then(|policy| match self.cache_scope() {
                    Ok(scope) => Some((policy, scope)),
                    Err(uncached) => {
                        uncached.log(&self.id().to_string());
                        None
                    }
                });
            let mut context = self.context.clone().with_collection_state(state);
            context.set_state_dir(cache.map(|_| cache_dir.to_owned()));
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
            match cache {
                Some((policy, scope)) => {
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
                None => fetch(options).await,
            }
        };
        diagnostics::in_connection(self.id().clone(), collect).await
    }

    /// Adds what its kind looks up now to this connection's collected `entries`
    /// ([`ProviderAdapter::annotate`]), its log lines shown under the connection.
    pub async fn annotate(&self, entries: &mut [ActivityEntry]) {
        diagnostics::in_connection(
            self.id().clone(),
            self.adapter.annotate(entries, self.context.clone()),
        )
        .await;
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
pub async fn inspect_provider_runtimes_with(
    connections: &[Connection],
    secrets: &Secrets,
) -> Vec<ProviderRuntime> {
    join_all(
        start_checks(connections, secrets)
            .into_iter()
            .map(|(_, check)| check),
    )
    .await
}

/// For each connection: what its info says while its startup check runs, and that check.
pub fn start_checks(
    connections: &[Connection],
    secrets: &Secrets,
) -> Vec<(ProviderInfo, BoxFuture<'static, ProviderRuntime>)> {
    connections
        .iter()
        .map(|connection| start_check(connection, secrets))
        .collect()
}

/// `start_checks` for one connection.
pub fn start_check(
    connection: &Connection,
    secrets: &Secrets,
) -> (ProviderInfo, BoxFuture<'static, ProviderRuntime>) {
    start_check_with(adapter(connection.source()).clone(), connection, secrets)
}

/// `start_check` with the adapter given. While the check waits for the OS store (a dialog the
/// user has not answered), the connection says so.
pub(crate) fn start_check_with(
    adapter: Arc<dyn ProviderAdapter>,
    connection: &Connection,
    secrets: &Secrets,
) -> (ProviderInfo, BoxFuture<'static, ProviderRuntime>) {
    let mut checking = provider_info(adapter.as_ref(), connection);
    checking.reason = secrets
        .waiting_reason(connection.values())
        .map(str::to_string);
    let connection = connection.clone();
    let secrets = secrets.clone();
    let check = async move { ProviderRuntime::inspect(adapter, &connection, &secrets).await };
    (checking, check.boxed())
}

async fn check_provider(
    adapter: &dyn ProviderAdapter,
    connection: &Connection,
    context: &mut ProviderContext,
    unopened: Option<&Unopened>,
) -> ProviderCheckResult {
    let current_os = current_os();
    let requirements = adapter.config_requirements();
    let started = std::time::Instant::now();
    let result = if !adapter.supported_os().contains(&current_os) {
        unavailable(ProviderProblem::Unsupported, "OS not supported")
    } else if let Some(unopened) = unopened {
        unavailable(unopened.problem(), unopened.reason())
    } else if !context.missing_required_keys(requirements).is_empty() {
        unavailable(
            ProviderProblem::NotConfigured,
            format!("{} is not configured", connection.label),
        )
    } else if let Some(problem) = context.first_config_problem(requirements) {
        unavailable(ProviderProblem::NotConfigured, problem)
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

    result
}

fn unavailable(problem: ProviderProblem, reason: impl Into<String>) -> ProviderCheckResult {
    ProviderCheckResult::unavailable_as(problem, reason)
}

/// `connection` as it shows while its check runs: not usable, no problem yet.
fn provider_info(adapter: &dyn ProviderAdapter, connection: &Connection) -> ProviderInfo {
    ProviderInfo {
        connection: connection.id.clone(),
        label: connection.label.clone(),
        wsl_distro: connection.location.wsl_distro().map(str::to_string),
        status: ProviderStatus::Checking,
        usable: false,
        supported_os: adapter.supported_os().to_vec(),
        reason: None,
        problem: None,
        fix_action: None,
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
