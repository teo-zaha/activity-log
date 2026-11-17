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
    collect::git::{
        GitCollection, RepoCommits, RepoHistory, resolve_git_repos_from_roots,
        resolve_git_repos_with_home_root,
    },
    diagnostics,
    model::{ActivityEntry, CollectOptions, ConnectionId, GitEntry, Source, SupportedOs},
    provider::{
        connection::{Connection, ConnectionLocation, ConnectionValues},
        current_os,
        git::{GIT_DIRS, GitConfig, GitProvider, default_git_root_with_lookup},
        provider_adapter::ProviderAdapter,
        provider_check_result::ProviderCheckResult,
        provider_config::{
            ConfigBinding, ConfigSource, ProviderConfig, ProviderConfigKey, ProviderConfigResolve,
            ProviderConfigShape, ProviderConfigValueType, parse_config_list,
            provider_config_key_accepts,
        },
        provider_context::ProviderContext,
        provider_registry,
        provider_runtime::{ProviderRuntime, with_startup_timeout},
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
            Source::Calendar,
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

/// A Git connection with these values.
fn test_connection(config: TestConfigSource) -> Connection {
    Connection::new(
        ConnectionId::new(Source::Git, "default"),
        "Git",
        ConnectionLocation::Local,
        ConnectionValues::new(
            config
                .values
                .into_iter()
                .map(|(key, value)| (key, Some(value)))
                .collect(),
        ),
    )
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
        &test_connection(TestConfigSource::default()),
    )
    .await;

    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(!runtime.info().usable);
    assert_eq!(
        runtime.info().reason.as_deref(),
        Some("Git is not configured")
    );
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
        &test_connection(TestConfigSource::default()),
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
        &test_connection(config),
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
        &test_connection(config),
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
        &test_connection(config),
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
        &test_connection(config),
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
        TEST_MACRO_BOOL_FLAG.value_type(),
        ProviderConfigValueType::Bool
    );

    assert_eq!(TEST_MACRO_DEFAULT_VALUE.name, "TEST_MACRO_DEFAULT_VALUE");
    assert!(!std::hint::black_box(TEST_MACRO_DEFAULT_VALUE.required));
    assert_eq!(
        TEST_MACRO_DEFAULT_VALUE.value_type(),
        ProviderConfigValueType::String
    );
}

#[test]
fn windows_git_default_root_prefers_user_profile() {
    let root = default_git_root_with_lookup(SupportedOs::Windows, |key| match key {
        "USERPROFILE" => Some(r"C:\Users\me".into()),
        _ => None,
    });

    assert_eq!(root, Some(PathBuf::from(r"C:\Users\me")));
}

#[test]
fn unix_git_default_root_uses_home() {
    let macos_root = default_git_root_with_lookup(SupportedOs::Macos, |key| match key {
        "HOME" => Some("/Users/me".into()),
        _ => None,
    });
    let linux_root = default_git_root_with_lookup(SupportedOs::Linux, |key| match key {
        "HOME" => Some("/home/me".into()),
        _ => None,
    });

    assert_eq!(macos_root, Some(PathBuf::from("/Users/me")));
    assert_eq!(linux_root, Some(PathBuf::from("/home/me")));
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

    let runtime = ProviderRuntime::inspect(Arc::new(GitProvider), &test_connection(config)).await;

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

    let runtime = ProviderRuntime::inspect(Arc::new(GitProvider), &test_connection(config)).await;

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

    let runtime = ProviderRuntime::inspect(Arc::new(GitProvider), &test_connection(config)).await;

    assert!(!runtime.info().usable);
    assert!(
        runtime
            .info()
            .reason
            .as_deref()
            .is_some_and(|reason| reason.starts_with("No readable repositories found:"))
    );
}

#[test]
fn git_repo_discovery_skips_dependency_and_appdata_folders() {
    let tmp = tempdir().expect("tempdir");
    let kept = tmp.path().join("code").join("app");
    std::fs::create_dir_all(kept.join(".git")).expect("kept repo");
    for skipped in ["node_modules", "AppData"] {
        std::fs::create_dir_all(tmp.path().join(skipped).join("pkg").join(".git"))
            .expect("skipped repo");
    }

    let repos = resolve_git_repos_from_roots(&[tmp.path().to_path_buf()]);

    assert_eq!(repos, vec![kept]);
}

#[tokio::test]
async fn git_provider_accepts_quoted_paths_from_windows_copy_as_path() {
    let repo = init_git_repo();
    let mut config = TestConfigSource::default();
    config.values.insert(
        "GIT_DIRS".to_string(),
        format!("\"{}\"", repo.path().display()),
    );

    let runtime = ProviderRuntime::inspect(Arc::new(GitProvider), &test_connection(config)).await;

    assert!(runtime.info().usable, "{:?}", runtime.info().reason);
}

#[tokio::test]
async fn startup_checks_that_hang_report_a_timeout() {
    let result = with_startup_timeout(
        std::future::pending::<ProviderCheckResult>(),
        std::time::Duration::from_millis(10),
    )
    .await;

    assert_eq!(result.status, crate::model::ProviderStatus::Unavailable);
    assert!(
        result
            .reason
            .as_deref()
            .is_some_and(|reason| reason.starts_with("Startup check timed out"))
    );
}

#[test]
fn only_remote_api_providers_opt_into_the_disk_cache() {
    use crate::provider::provider_registry;

    let cached: Vec<Source> = provider_registry()
        .iter()
        .filter(|adapter| adapter.cache_policy().is_some())
        .map(|adapter| adapter.id())
        .collect();

    assert_eq!(cached, [Source::Gitlab, Source::Jira]);
}

#[test]
fn cache_fingerprint_separates_accounts_but_ignores_secrets() {
    const KEYS: &[ProviderConfigKey] = &[
        ProviderConfigKey::required("URL", ProviderConfigValueType::Url),
        ProviderConfigKey::required("USER", ProviderConfigValueType::String),
        ProviderConfigKey::required("TOKEN", ProviderConfigValueType::Password),
    ];
    let fingerprint = |user: &str, token: &str| {
        let mut source = TestConfigSource::default();
        source
            .values
            .insert("URL".into(), "https://gitlab.example".into());
        source.values.insert("USER".into(), user.into());
        source.values.insert("TOKEN".into(), token.into());
        ProviderConfig::resolve(&source, KEYS).fingerprint(KEYS)
    };

    assert_eq!(fingerprint("alice", "one"), fingerprint("alice", "two"));
    assert_ne!(fingerprint("alice", "one"), fingerprint("bob", "one"));
}

#[test]
fn config_lists_read_json_arrays_and_comma_lists() {
    assert_eq!(
        parse_config_list(r#"["C:\\a, b", " D:\\c "]"#),
        ["C:\\a, b", "D:\\c"]
    );
    assert_eq!(parse_config_list(r#" "C:\a" , ,D:\b"#), ["C:\\a", "D:\\b"]);
    assert!(parse_config_list("[]").is_empty());
}

struct MapSource(Vec<(&'static str, &'static str)>);

impl ConfigSource for MapSource {
    fn get(&self, key: &str) -> Option<String> {
        self.0
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| value.to_string())
    }
}

fn git_config(values: Vec<(&'static str, &'static str)>) -> anyhow::Result<GitConfig> {
    GitConfig::from_config(&ProviderConfig::resolve(&MapSource(values), &[GIT_DIRS]))
}

#[test]
fn git_config_tells_unset_lists_from_empty_ones() {
    // `GIT_DIRS=` in a .env file is unset, so the default applies.
    assert_eq!(git_config(vec![("GIT_DIRS", "")]).unwrap().dirs, None);
    // A saved empty list means none.
    assert_eq!(
        git_config(vec![("GIT_DIRS", "[]")]).unwrap().dirs,
        Some(Vec::new())
    );
}

#[test]
fn list_keys_validate_every_item_against_their_value_type() {
    let ports = ProviderConfigKey::optional_int("TEST_PORTS").list_of();

    assert_eq!(
        ports.shape,
        ProviderConfigShape::List(ProviderConfigValueType::Int)
    );
    assert_eq!(ports.shape.to_string(), "list of int");
    assert!(provider_config_key_accepts(ports, "80, 443"));
    assert!(provider_config_key_accepts(ports, r#"["80","443"]"#));
    assert!(!provider_config_key_accepts(ports, "80,https"));
    assert_eq!(
        GIT_DIRS.shape,
        ProviderConfigShape::List(ProviderConfigValueType::String)
    );
}

#[test]
fn reads_wsl_folders() {
    assert_eq!(
        crate::provider::git::parse_wsl_dir(" ~/projects/ ").unwrap(),
        "~/projects"
    );
    assert_eq!(
        crate::provider::git::parse_wsl_dir("/home/nixos/trn/atlas/").unwrap(),
        "/home/nixos/trn/atlas"
    );
    assert_eq!(crate::provider::git::parse_wsl_dir("~").unwrap(), "~");
    assert_eq!(crate::provider::git::parse_wsl_dir("/").unwrap(), "/");
}

#[test]
fn rejects_folders_that_are_not_linux_paths() {
    assert!(crate::provider::git::parse_wsl_dir("C:/work/repo").is_err());
    assert!(crate::provider::git::parse_wsl_dir("relative/path").is_err());
    assert!(crate::provider::git::parse_wsl_dir("~user").is_err());
}

fn commit(hash: &str, author: &str) -> ActivityEntry {
    ActivityEntry::Git(GitEntry::new(
        "repo".to_string(),
        hash.to_string(),
        "message".to_string(),
        author.to_string(),
        "main".to_string(),
        "2026-03-05T09:00:00Z".to_string(),
    ))
}

fn hashes(results: &[(ConnectionId, Vec<ActivityEntry>)]) -> Vec<Vec<&str>> {
    results
        .iter()
        .map(|(_, entries)| {
            entries
                .iter()
                .map(|entry| match entry {
                    ActivityEntry::Git(commit) => commit.hash.as_str(),
                    other => panic!("unexpected entry {other:?}"),
                })
                .collect()
        })
        .collect()
}

/// Records what a connection found whose repository has `email` set.
fn record_email(collection: &GitCollection, email: &str) {
    collection.record(&RepoCommits {
        repos: vec![RepoHistory {
            label: "repo".to_string(),
            email: Some(email.to_string()),
            commits: Vec::new(),
        }],
        identities: Vec::new(),
    });
}

#[tokio::test]
async fn a_provisional_git_pass_keeps_a_subset_and_logs_nothing() {
    // Scopes this test's log lines, so other tests' lines do not count.
    let scope = ConnectionId::new(Source::Git, "provisional-pass");
    let logged = || {
        diagnostics::entries()
            .into_iter()
            .filter(|entry| entry.scope == "git/provisional-pass")
            .map(|entry| entry.message)
            .collect::<Vec<_>>()
    };
    let first = (
        ConnectionId::new(Source::Git, "first"),
        vec![
            commit("a1", "me@example.com"),
            commit("b2", "alias@example.com"),
        ],
    );
    let second = (
        ConnectionId::new(Source::Git, "second"),
        vec![
            commit("a1", "me@example.com"),
            commit("c3", "alias@example.com"),
        ],
    );

    diagnostics::in_connection(scope, async {
        let state = GitProvider.collection_state();
        let collection = state.as_ref().unwrap().downcast_ref().unwrap();

        record_email(collection, "me@example.com");
        let provisional = GitProvider.finish_kind(state.as_ref(), vec![first.clone()], false);
        assert_eq!(hashes(&provisional), [["a1"]]);
        assert_eq!(logged(), Vec::<String>::new());

        record_email(collection, "alias@example.com");
        let complete = GitProvider.finish_kind(state.as_ref(), vec![first, second], true);
        assert_eq!(hashes(&complete), [vec!["a1", "b2"], vec!["c3"]]);
        assert_eq!(
            logged(),
            [
                "Keeping commits by alias@example.com, me@example.com",
                "Showed 1 commits once that more than one clone holds",
            ]
        );
    })
    .await;
}
