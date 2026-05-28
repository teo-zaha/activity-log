use futures::future::BoxFuture;
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use crate::{
    collect::git::{resolve_git_repos_from_roots, resolve_git_repos_with_home_root},
    model::{ActivityEntry, CollectOptions, Source, SupportedOs},
    provider::{
        current_os,
        git::{GitConfig, GitProvider, default_git_root_with_lookup},
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
use tempfile::tempdir;

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
    let _ = MacroBoolConfig::binding();
    let source = TestConfigSource::default();
    let config = ProviderConfig::resolve(&source, MacroBoolConfig::requirements());
    let parsed = MacroBoolConfig::from_config(&config).expect("macro bool config should parse");
    assert!(!parsed.flag);
}

#[test]
fn provider_config_macro_default_string_is_used_when_missing() {
    let _ = MacroDefaultStringConfig::binding();
    let source = TestConfigSource::default();
    let config = ProviderConfig::resolve(&source, MacroDefaultStringConfig::requirements());
    let parsed =
        MacroDefaultStringConfig::from_config(&config).expect("macro default config should parse");
    assert_eq!(parsed.value, "fallback");
}

#[test]
fn provider_config_macro_generates_expected_key_metadata() {
    assert_eq!(TEST_MACRO_BOOL_FLAG.name, "TEST_MACRO_BOOL_FLAG");
    assert!(!std::hint::black_box(TEST_MACRO_BOOL_FLAG.required));
    assert_eq!(
        TEST_MACRO_BOOL_FLAG.value_type,
        ProviderConfigValueType::Bool
    );

    assert_eq!(TEST_MACRO_DEFAULT_VALUE.name, "TEST_MACRO_DEFAULT_VALUE");
    assert!(!std::hint::black_box(TEST_MACRO_DEFAULT_VALUE.required));
    assert_eq!(
        TEST_MACRO_DEFAULT_VALUE.value_type,
        ProviderConfigValueType::String
    );
}

#[test]
fn windows_git_default_root_prefers_user_profile() {
    let root = default_git_root_with_lookup(SupportedOs::Windows, |key| match key {
        "USERPROFILE" => Some(r"C:\Users\teodora".into()),
        _ => None,
    });

    assert_eq!(root, Some(PathBuf::from(r"C:\Users\teodora")));
}

#[test]
fn unix_git_default_root_uses_home() {
    let macos_root = default_git_root_with_lookup(SupportedOs::Macos, |key| match key {
        "HOME" => Some("/Users/teodora".into()),
        _ => None,
    });
    let linux_root = default_git_root_with_lookup(SupportedOs::Linux, |key| match key {
        "HOME" => Some("/home/teodora".into()),
        _ => None,
    });

    assert_eq!(macos_root, Some(PathBuf::from("/Users/teodora")));
    assert_eq!(linux_root, Some(PathBuf::from("/home/teodora")));
}

#[test]
fn unix_git_default_root_does_not_fall_back_to_root() {
    let root = default_git_root_with_lookup(SupportedOs::Linux, |_| None);
    assert_eq!(root, None);
}

#[test]
fn git_resolution_uses_home_discovery_when_dirs_are_unset() {
    let tmp = tempdir().expect("tempdir");
    let home_root = tmp.path().join("home");
    let nested_repo = home_root.join("workspace").join("nested-repo");

    std::fs::create_dir_all(nested_repo.join(".git")).expect("nested repo");

    let repos = resolve_git_repos_with_home_root(&GitConfig { dirs: None }, Some(home_root))
        .expect("home discovery should succeed");

    assert_eq!(repos, vec![nested_repo]);
}

#[test]
fn git_repo_resolution_accepts_repo_paths_and_search_roots() {
    let tmp = tempdir().expect("tempdir");
    let direct_repo = tmp.path().join("direct-repo");
    let nested_root = tmp.path().join("workspace");
    let nested_repo = nested_root.join("nested-repo");
    let duplicate_root = nested_repo.clone();

    std::fs::create_dir_all(direct_repo.join(".git")).expect("direct repo");
    std::fs::create_dir_all(nested_repo.join(".git")).expect("nested repo");

    let repos = resolve_git_repos_from_roots(&[
        direct_repo.clone(),
        nested_root,
        duplicate_root,
        tmp.path().join("missing"),
    ]);

    assert_eq!(repos, vec![direct_repo, nested_repo]);
}

fn run_git(repo: &std::path::Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

fn init_git_repo() -> tempfile::TempDir {
    let dir = tempdir().expect("tempdir");
    run_git(dir.path(), &["init"]);
    run_git(dir.path(), &["checkout", "-b", "main"]);
    dir
}

#[tokio::test]
async fn git_provider_is_unavailable_when_no_repositories_are_found() {
    let empty_root = tempdir().expect("tempdir");
    let mut config = TestConfigSource::default();
    config.values.insert(
        "GIT_DIRS".to_string(),
        empty_root.path().display().to_string(),
    );

    let runtime = ProviderRuntime::inspect(Arc::new(GitProvider), &config).await;

    assert!(!runtime.info().usable);
    assert_eq!(
        runtime.info().reason.as_deref(),
        Some("No repositories found")
    );
}

#[tokio::test]
async fn git_provider_is_available_when_a_repository_can_be_opened() {
    let repo = init_git_repo();
    let mut config = TestConfigSource::default();
    config
        .values
        .insert("GIT_DIRS".to_string(), repo.path().display().to_string());

    let runtime = ProviderRuntime::inspect(Arc::new(GitProvider), &config).await;

    assert!(runtime.info().usable);
}

#[tokio::test]
async fn git_provider_is_unavailable_when_discovered_repositories_are_invalid() {
    let invalid_root = tempdir().expect("tempdir");
    fs::create_dir_all(invalid_root.path().join(".git")).expect("fake git dir");

    let mut config = TestConfigSource::default();
    config.values.insert(
        "GIT_DIRS".to_string(),
        invalid_root.path().display().to_string(),
    );

    let runtime = ProviderRuntime::inspect(Arc::new(GitProvider), &config).await;

    assert!(!runtime.info().usable);
    assert!(
        runtime
            .info()
            .reason
            .as_deref()
            .is_some_and(|reason| reason.starts_with("No readable repositories found:"))
    );
}
