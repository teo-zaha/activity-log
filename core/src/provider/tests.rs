use futures::future::BoxFuture;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use crate::{
    model::{CollectOptions, Source, SupportedOs},
    provider::{
        current_os,
        provider_adapter::ProviderAdapter,
        provider_check_result::ProviderCheckResult,
        provider_collection::ProviderCollection,
        provider_config::{ConfigSource, ProviderConfigKey, ProviderConfigValueType},
        provider_context::ProviderContext,
        provider_registry,
        provider_runtime::ProviderRuntime,
    },
};

#[test]
fn registry_includes_all_builtin_providers() {
    let ids = provider_registry()
        .into_iter()
        .map(|adapter| adapter.id())
        .collect::<Vec<_>>();

    assert_eq!(
        ids,
        vec![
            Source::Git,
            Source::Gitlab,
            Source::Jira,
            Source::Outlook,
            Source::System,
        ]
    );
}

#[test]
fn outlook_and_system_declare_windows_and_macos_support() {
    let adapters = provider_registry();
    let desktop_native = adapters
        .into_iter()
        .filter(|adapter| matches!(adapter.id(), Source::Outlook | Source::System))
        .collect::<Vec<_>>();

    assert!(
        desktop_native
            .iter()
            .all(|adapter| adapter.supported_os() == [SupportedOs::Windows, SupportedOs::Macos])
    );
}

#[derive(Default)]
struct TestConfigSource {
    values: std::collections::BTreeMap<String, String>,
}

impl ConfigSource for TestConfigSource {
    fn get(&self, key: &str) -> Option<String> {
        self.values.get(key).cloned()
    }
}

struct TestProvider {
    supported_os: &'static [SupportedOs],
    requirements: &'static [ProviderConfigKey],
    startup_result: ProviderCheckResult,
    calls: Arc<AtomicUsize>,
}

impl ProviderAdapter for TestProvider {
    fn id(&self) -> Source {
        Source::Git
    }

    fn supported_os(&self) -> &'static [SupportedOs] {
        self.supported_os
    }

    fn config_requirements(&self) -> &'static [ProviderConfigKey] {
        self.requirements
    }

    fn startup_check(
        &self,
        _current_os: SupportedOs,
        _context: ProviderContext,
    ) -> BoxFuture<'_, ProviderCheckResult> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let result = self.startup_result.clone();
        Box::pin(async move { result })
    }

    fn collect(
        &self,
        _current_os: SupportedOs,
        _options: CollectOptions,
        _context: ProviderContext,
    ) -> BoxFuture<'_, anyhow::Result<ProviderCollection>> {
        Box::pin(async { unreachable!("collect is not exercised in provider runtime tests") })
    }
}

#[tokio::test]
async fn missing_required_config_skips_startup_check() {
    static REQUIRED_KEYS: &[ProviderConfigKey] = &[ProviderConfigKey::required_string("TEST_KEY")];

    let calls = Arc::new(AtomicUsize::new(0));
    let runtime = ProviderRuntime::inspect(
        Arc::new(TestProvider {
            supported_os: &[SupportedOs::Windows, SupportedOs::Macos, SupportedOs::Linux],
            requirements: REQUIRED_KEYS,
            startup_result: ProviderCheckResult::available(),
            calls: Arc::clone(&calls),
        }),
        &TestConfigSource::default(),
    )
    .await;

    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(!runtime.info().usable);
    assert_eq!(runtime.info().reason.as_deref(), Some("git not configured"));
}

#[tokio::test]
async fn unsupported_os_is_derived_from_provider_metadata() {
    let unsupported_os = match current_os() {
        SupportedOs::Windows => &[SupportedOs::Linux][..],
        SupportedOs::Macos => &[SupportedOs::Windows][..],
        SupportedOs::Linux => &[SupportedOs::Windows][..],
    };
    let calls = Arc::new(AtomicUsize::new(0));
    let runtime = ProviderRuntime::inspect(
        Arc::new(TestProvider {
            supported_os: unsupported_os,
            requirements: &[],
            startup_result: ProviderCheckResult::available(),
            calls: Arc::clone(&calls),
        }),
        &TestConfigSource::default(),
    )
    .await;

    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(runtime.info().reason.as_deref(), Some("OS not supported"));
}

#[tokio::test]
async fn startup_check_runs_with_resolved_config() {
    static REQUIRED_KEYS: &[ProviderConfigKey] = &[ProviderConfigKey::required_string("TEST_KEY")];

    let calls = Arc::new(AtomicUsize::new(0));
    let mut config = TestConfigSource::default();
    config
        .values
        .insert("TEST_KEY".to_string(), "configured".to_string());

    let runtime = ProviderRuntime::inspect(
        Arc::new(TestProvider {
            supported_os: &[SupportedOs::Windows, SupportedOs::Macos, SupportedOs::Linux],
            requirements: REQUIRED_KEYS,
            startup_result: ProviderCheckResult::available(),
            calls: Arc::clone(&calls),
        }),
        &config,
    )
    .await;

    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(runtime.info().usable);
}

#[tokio::test]
async fn invalid_typed_config_skips_startup_check() {
    static REQUIRED_KEYS: &[ProviderConfigKey] = &[ProviderConfigKey::required(
        "TEST_URL",
        ProviderConfigValueType::Url,
    )];

    let calls = Arc::new(AtomicUsize::new(0));
    let mut config = TestConfigSource::default();
    config
        .values
        .insert("TEST_URL".to_string(), "not-a-url".to_string());

    let runtime = ProviderRuntime::inspect(
        Arc::new(TestProvider {
            supported_os: &[SupportedOs::Windows, SupportedOs::Macos, SupportedOs::Linux],
            requirements: REQUIRED_KEYS,
            startup_result: ProviderCheckResult::available(),
            calls: Arc::clone(&calls),
        }),
        &config,
    )
    .await;

    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(!runtime.info().usable);
    assert_eq!(
        runtime.info().reason.as_deref(),
        Some("TEST_URL must be a url")
    );
}

#[tokio::test]
async fn strict_bool_config_rejects_non_true_false_values() {
    static REQUIRED_KEYS: &[ProviderConfigKey] = &[ProviderConfigKey::required_bool("TEST_BOOL")];

    let calls = Arc::new(AtomicUsize::new(0));
    let mut config = TestConfigSource::default();
    config
        .values
        .insert("TEST_BOOL".to_string(), "1".to_string());

    let runtime = ProviderRuntime::inspect(
        Arc::new(TestProvider {
            supported_os: &[SupportedOs::Windows, SupportedOs::Macos, SupportedOs::Linux],
            requirements: REQUIRED_KEYS,
            startup_result: ProviderCheckResult::available(),
            calls: Arc::clone(&calls),
        }),
        &config,
    )
    .await;

    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(!runtime.info().usable);
    assert_eq!(
        runtime.info().reason.as_deref(),
        Some("TEST_BOOL must be a bool")
    );
}
