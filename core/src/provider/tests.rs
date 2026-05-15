use futures::future::BoxFuture;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use crate::{
    model::{ActivityEntry, CollectOptions, Source, SupportedOs},
    provider::{
        current_os,
        provider_adapter::ProviderAdapter,
        provider_check_result::ProviderCheckResult,
        provider_config::{
            ConfigBinding, ConfigSource, ProviderConfig, ProviderConfigKey, ProviderConfigResolve,
            ProviderConfigValueType,
        },
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

fn noop_resolve(_context: &mut ProviderContext) -> anyhow::Result<()> {
    Ok(())
}

impl ProviderAdapter for TestProvider {
    fn id(&self) -> Source {
        Source::Git
    }

    fn supported_os(&self) -> &'static [SupportedOs] {
        self.supported_os
    }

    fn config_binding(&self) -> Option<ConfigBinding> {
        Some(ConfigBinding {
            requirements: self.requirements,
            resolve: noop_resolve,
        })
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
    ) -> BoxFuture<'_, anyhow::Result<Vec<ActivityEntry>>> {
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

#[derive(Debug, Clone)]
struct TypedTestConfig {
    value: String,
}

static TYPED_KEYS: &[ProviderConfigKey] = &[ProviderConfigKey::required_string("TYPED_KEY")];

impl ProviderConfigResolve for TypedTestConfig {
    fn requirements() -> &'static [ProviderConfigKey] {
        TYPED_KEYS
    }

    fn from_config(config: &ProviderConfig) -> anyhow::Result<Self> {
        Ok(Self {
            value: config.require(TYPED_KEYS[0])?.to_string(),
        })
    }
}

impl TypedTestConfig {
    fn binding() -> ConfigBinding {
        ConfigBinding::of::<Self>()
    }
}

struct TypedTestProvider {
    calls: Arc<AtomicUsize>,
}

impl ProviderAdapter for TypedTestProvider {
    fn id(&self) -> Source {
        Source::Git
    }

    fn supported_os(&self) -> &'static [SupportedOs] {
        &[SupportedOs::Windows, SupportedOs::Macos, SupportedOs::Linux]
    }

    fn config_binding(&self) -> Option<ConfigBinding> {
        Some(TypedTestConfig::binding())
    }

    fn startup_check(
        &self,
        _current_os: SupportedOs,
        context: ProviderContext,
    ) -> BoxFuture<'_, ProviderCheckResult> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            let config = context
                .config::<TypedTestConfig>()
                .expect("typed config should be resolved");
            assert_eq!(config.value, "hello");
            ProviderCheckResult::available()
        })
    }

    fn collect(
        &self,
        _current_os: SupportedOs,
        _options: CollectOptions,
        _context: ProviderContext,
    ) -> BoxFuture<'_, anyhow::Result<Vec<ActivityEntry>>> {
        Box::pin(async { unreachable!() })
    }
}

#[tokio::test]
async fn typed_config_is_available_in_startup_check() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut config = TestConfigSource::default();
    config
        .values
        .insert("TYPED_KEY".to_string(), "hello".to_string());

    let runtime = ProviderRuntime::inspect(
        Arc::new(TypedTestProvider {
            calls: Arc::clone(&calls),
        }),
        &config,
    )
    .await;

    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(runtime.info().usable);
}

crate::provider_config! {
    struct MacroBoolConfig {
        flag => TEST_MACRO_BOOL_FLAG: Bool,
    }
}

crate::provider_config! {
    struct MacroDefaultStringConfig {
        value => TEST_MACRO_DEFAULT_VALUE: String = "fallback",
    }
}

#[test]
fn provider_config_macro_bool_defaults_to_false() {
    let source = TestConfigSource::default();
    let config = ProviderConfig::resolve(&source, MacroBoolConfig::requirements());
    let parsed = MacroBoolConfig::from_config(&config).expect("macro bool config should parse");
    assert!(!parsed.flag);
}

#[test]
fn provider_config_macro_default_string_is_used_when_missing() {
    let source = TestConfigSource::default();
    let config = ProviderConfig::resolve(&source, MacroDefaultStringConfig::requirements());
    let parsed =
        MacroDefaultStringConfig::from_config(&config).expect("macro default config should parse");
    assert_eq!(parsed.value, "fallback");
}

#[test]
fn provider_config_macro_generates_expected_key_metadata() {
    assert_eq!(TEST_MACRO_BOOL_FLAG.name, "TEST_MACRO_BOOL_FLAG");
    assert!(!TEST_MACRO_BOOL_FLAG.required);
    assert_eq!(TEST_MACRO_BOOL_FLAG.value_type, ProviderConfigValueType::Bool);

    assert_eq!(TEST_MACRO_DEFAULT_VALUE.name, "TEST_MACRO_DEFAULT_VALUE");
    assert!(!TEST_MACRO_DEFAULT_VALUE.required);
    assert_eq!(
        TEST_MACRO_DEFAULT_VALUE.value_type,
        ProviderConfigValueType::String
    );
}
