use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use super::{Place, hint, import_once, marker::MARKER};
use crate::{
    diagnostics,
    model::{LegacyImportReportDto, LegacySkipReason, LegacySource, LegacyStoreWait, Source},
    provider::{
        connection::{StoredSecret, StoredValue},
        current_os,
    },
    secrets::{
        DataKey, FrontEnd, MemoryStore, SecretStore, Secrets, StoreCall, StoreErrorKind, StoreKind,
    },
    settings::ConfigFile,
};

const TOKEN: &str = "glpat-test-0000";
const URL: &str = "https://gitlab.example.test";

struct Fixture {
    _dir: tempfile::TempDir,
    root: PathBuf,
    place: Place,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let work = root.join("work");
        fs::create_dir_all(root.join("legacy")).unwrap();
        fs::create_dir_all(&work).unwrap();
        let place = Place {
            config: root.join("config").join("config.toml"),
            settings: Some(root.join("legacy").join("settings.json")),
            env_from: Some(work),
            os: current_os(),
            wsl_distros: Vec::new,
        };
        Self {
            _dir: dir,
            root,
            place,
        }
    }

    fn settings(&self, provider_config: serde_json::Value) -> PathBuf {
        let path = self.place.settings.clone().unwrap();
        let store = serde_json::json!({ "providerConfig": provider_config });
        fs::write(&path, serde_json::to_vec(&store).unwrap()).unwrap();
        path
    }

    fn env(&self, text: &str) -> PathBuf {
        let path = self.root.join(".env");
        fs::write(&path, text).unwrap();
        path
    }

    fn config(&self) -> Option<String> {
        fs::read_to_string(&self.place.config).ok()
    }

    fn marker(&self) -> PathBuf {
        self.place.config.with_file_name(MARKER)
    }
}

/// The stores of `front_end`: an OS store holding nothing (or none), a key file.
fn secrets(
    front_end: FrontEnd,
    with_os: bool,
    root: &Path,
) -> (Secrets, Arc<MemoryStore>, Arc<MemoryStore>) {
    let os = Arc::new(MemoryStore::new(StoreKind::Os));
    let file = Arc::new(MemoryStore::new(StoreKind::File));
    let secrets = Secrets::with_stores(
        front_end,
        with_os.then(|| {
            (
                os.clone() as Arc<dyn SecretStore>,
                Some(root.join("secret-key.lock")),
            )
        }),
        (
            file.clone() as Arc<dyn SecretStore>,
            Some(root.join("secret.key.lock")),
        ),
    );
    (secrets, os, file)
}

fn sealed_store(config: &ConfigFile, source: Source, key: &str) -> Option<StoreKind> {
    config
        .connections()
        .connections
        .into_iter()
        .find(|connection| *connection.source() == source)
        .and_then(|connection| match connection.values().get_stored(key) {
            Some(StoredValue::Sealed(StoredSecret::Sealed(value))) => Some(value.store()),
            _ => None,
        })
}

fn gitlab_settings() -> serde_json::Value {
    serde_json::json!({ "GITLAB_URL": URL, "GITLAB_TOKEN": TOKEN, "GITLAB_USER": "me" })
}

/// The import writes the sources once, the token sealed in the OS store, and the marker; the
/// legacy file stays byte for byte and untouched; a second start does nothing.
#[tokio::test(flavor = "multi_thread")]
async fn settings_are_imported_once_and_the_legacy_file_is_left_alone() {
    let fixture = Fixture::new();
    let legacy = fixture.settings(gitlab_settings());
    let before = fs::read(&legacy).unwrap();
    let modified = fs::metadata(&legacy).unwrap().modified().unwrap();
    let (secrets, os, _) = secrets(FrontEnd::Desktop, true, &fixture.root);

    let report = import_once(&fixture.place, &secrets).await.unwrap();
    let LegacyImportReportDto::Imported {
        file,
        unused,
        imported,
        skipped,
        ..
    } = report
    else {
        panic!("{report:?}");
    };
    assert_eq!(file.source, LegacySource::Settings);
    assert_eq!(unused, None);
    assert!(skipped.is_empty());
    assert!(imported.iter().any(|id| id.source == Source::Gitlab));
    assert!(imported.iter().any(|id| id.source == Source::Git));

    let text = fixture.config().unwrap();
    assert!(!text.contains(TOKEN), "{text}");
    let config = ConfigFile::load(&fixture.place.config).unwrap();
    assert_eq!(
        sealed_store(&config, Source::Gitlab, "GITLAB_TOKEN"),
        Some(StoreKind::Os)
    );
    assert!(os.key().is_some());
    assert!(fixture.marker().is_file());
    assert_eq!(fs::read(&legacy).unwrap(), before);
    assert_eq!(fs::metadata(&legacy).unwrap().modified().unwrap(), modified);

    assert_eq!(import_once(&fixture.place, &secrets).await, None);
    assert_eq!(fixture.config().unwrap(), text);
}

/// No import over a `config.toml`, an empty one included, or beside a marker, or with nothing
/// found.
#[tokio::test(flavor = "multi_thread")]
async fn an_import_runs_only_where_it_is_due() {
    let (secrets, _, _) = secrets(FrontEnd::Desktop, true, Path::new("/nonexistent"));

    let nothing = Fixture::new();
    assert_eq!(import_once(&nothing.place, &secrets).await, None);
    assert_eq!(nothing.config(), None);

    let empty = Fixture::new();
    empty.settings(gitlab_settings());
    fs::create_dir_all(empty.place.config.parent().unwrap()).unwrap();
    fs::write(&empty.place.config, "").unwrap();
    assert_eq!(import_once(&empty.place, &secrets).await, None);
    assert_eq!(empty.config().unwrap(), "");

    let marked = Fixture::new();
    marked.settings(gitlab_settings());
    fs::create_dir_all(marked.place.config.parent().unwrap()).unwrap();
    fs::write(marked.marker(), "release = \"0.0.7\"\n").unwrap();
    assert_eq!(import_once(&marked.place, &secrets).await, None);
    assert_eq!(marked.config(), None);

    // A config path given, or the dev profile: no place at all.
    assert!(Place::for_front_end(FrontEnd::Cli, Some(Path::new("/x/config.toml"))).is_none());
}

/// `settings.json` wins only where it sets a key 0.0.7 read; the other file is named.
#[tokio::test(flavor = "multi_thread")]
async fn settings_json_wins_only_when_it_sets_a_key() {
    let both = Fixture::new();
    both.settings(serde_json::json!({ "GITLAB_USER": "from-settings" }));
    let env = both.env("GITLAB_USER=from-env\n");
    let (secrets, _, _) = secrets(FrontEnd::Cli, true, &both.root);
    let Some(LegacyImportReportDto::Imported { file, unused, .. }) =
        import_once(&both.place, &secrets).await
    else {
        panic!("not imported");
    };
    assert_eq!(file.source, LegacySource::Settings);
    assert_eq!(unused.unwrap().path, env.display().to_string());
    assert!(both.config().unwrap().contains("from-settings"));

    let unset = Fixture::new();
    unset.settings(serde_json::json!({ "GITLAB_USER": null, "GITLAB_URL": " ", "OTHER": "x" }));
    unset.env("GITLAB_USER=from-env\n");
    let Some(LegacyImportReportDto::Imported { file, unused, .. }) =
        import_once(&unset.place, &secrets).await
    else {
        panic!("not imported");
    };
    assert_eq!(file.source, LegacySource::Env);
    assert_eq!(unused.unwrap().source, LegacySource::Settings);
    assert!(unset.config().unwrap().contains("from-env"));
}

/// The desktop writes a token only into the OS store: without one, or refused, nothing is
/// written and no marker, so the next start tries again. Without a token it needs none.
#[tokio::test(flavor = "multi_thread")]
async fn the_desktop_waits_for_its_store() {
    let missing = Fixture::new();
    missing.settings(gitlab_settings());
    let (secrets_none, _, file) = secrets(FrontEnd::Desktop, false, &missing.root);
    let report = import_once(&missing.place, &secrets_none).await.unwrap();
    assert!(
        matches!(
            report,
            LegacyImportReportDto::Postponed {
                store: LegacyStoreWait::Unavailable,
                ..
            }
        ),
        "{report:?}"
    );
    assert_eq!(missing.config(), None);
    assert!(!missing.marker().exists());
    assert_eq!(file.calls().creates, 0);

    let refused = Fixture::new();
    refused.settings(gitlab_settings());
    let (secrets, os, _) = secrets(FrontEnd::Desktop, true, &refused.root);
    os.fail_next(StoreCall::Read, StoreErrorKind::Refused);
    let report = import_once(&refused.place, &secrets).await.unwrap();
    assert!(matches!(
        report,
        LegacyImportReportDto::Postponed {
            store: LegacyStoreWait::Refused,
            ..
        }
    ));
    assert_eq!(refused.config(), None);
    // The next start imports.
    assert!(matches!(
        import_once(&refused.place, &secrets).await,
        Some(LegacyImportReportDto::Imported { .. })
    ));

    let no_token = Fixture::new();
    no_token.settings(serde_json::json!({ "GITLAB_USER": "me" }));
    assert!(matches!(
        import_once(&no_token.place, &secrets_none).await,
        Some(LegacyImportReportDto::Imported { .. })
    ));
}

/// The CLI falls back to the key file where there is no OS store, and waits where it refuses.
#[tokio::test(flavor = "multi_thread")]
async fn the_cli_falls_back_to_the_key_file_but_not_past_a_refusal() {
    let fallback = Fixture::new();
    fallback.settings(gitlab_settings());
    let (secrets_file, _, file) = secrets(FrontEnd::Cli, false, &fallback.root);
    assert!(matches!(
        import_once(&fallback.place, &secrets_file).await,
        Some(LegacyImportReportDto::Imported { .. })
    ));
    let config = ConfigFile::load(&fallback.place.config).unwrap();
    assert_eq!(
        sealed_store(&config, Source::Gitlab, "GITLAB_TOKEN"),
        Some(StoreKind::File)
    );
    assert!(file.key().is_some());

    let refused = Fixture::new();
    refused.settings(gitlab_settings());
    let (secrets, os, _) = secrets(FrontEnd::Cli, true, &refused.root);
    os.fail_next(StoreCall::Read, StoreErrorKind::Refused);
    assert!(matches!(
        import_once(&refused.place, &secrets).await,
        Some(LegacyImportReportDto::Postponed {
            store: LegacyStoreWait::Refused,
            ..
        })
    ));
    assert_eq!(refused.config(), None);
}

/// Another process that writes `config.toml` while the key is read wins: the import is
/// dropped and its file is left as that process wrote it.
#[tokio::test(flavor = "multi_thread")]
async fn a_config_another_process_wrote_first_wins() {
    let fixture = Fixture::new();
    fixture.settings(gitlab_settings());
    let os = Arc::new(MemoryStore::holding(
        StoreKind::Os,
        DataKey::generate().unwrap(),
    ));
    let file: Arc<dyn SecretStore> = Arc::new(MemoryStore::new(StoreKind::File));
    let secrets = Secrets::with_stores(
        FrontEnd::Desktop,
        Some((os.clone() as Arc<dyn SecretStore>, None)),
        (file, None),
    );
    let gate = os.hold_reads();
    let place = fixture.place.clone();
    let import = tokio::spawn(async move { import_once(&place, &secrets).await });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let theirs = "[[git]]\nname = \"theirs\"\nlabel = \"Theirs\"\n";
    fs::create_dir_all(fixture.place.config.parent().unwrap()).unwrap();
    fs::write(&fixture.place.config, theirs).unwrap();
    gate.release();

    assert_eq!(import.await.unwrap(), None);
    assert_eq!(fixture.config().unwrap(), theirs);
    assert!(!fixture.marker().exists());
}

/// What cannot be written is left out and reported by connection and field; no value of the
/// legacy file reaches a log line or the report.
#[tokio::test(flavor = "multi_thread")]
async fn what_is_left_out_is_reported_without_values() {
    let fixture = Fixture::new();
    let secret = "glpat-left-out-0000";
    fixture.settings(serde_json::json!({
        "GITLAB_TOKEN": secret,
        "JIRA_URL": "ftp://jira.example.test",
        "JIRA_TOKEN": "jira-left-out-0000",
        "UNKNOWN_KEY": "unknown-value-0000",
    }));
    let (secrets, os, _) = secrets(FrontEnd::Desktop, true, &fixture.root);
    let report = import_once(&fixture.place, &secrets).await.unwrap();
    let LegacyImportReportDto::Imported { skipped, .. } = &report else {
        panic!("{report:?}");
    };
    let reasons = skipped
        .iter()
        .map(|skip| (skip.field.as_str(), skip.reason))
        .collect::<Vec<_>>();
    assert_eq!(
        reasons,
        [
            ("GITLAB_TOKEN", LegacySkipReason::NoAddress),
            ("JIRA_URL", LegacySkipReason::Invalid),
            ("JIRA_TOKEN", LegacySkipReason::NoAddress),
        ]
    );
    // No token was written, so no key was needed.
    assert_eq!(os.calls().reads, 0);
    let marker = fs::read_to_string(fixture.marker()).unwrap();
    assert!(marker.contains("noAddress"), "{marker}");

    let said = serde_json::to_string(&report).unwrap()
        + &marker
        + &fixture.config().unwrap()
        + &diagnostics::entries()
            .iter()
            .map(|entry| entry.message.clone())
            .collect::<String>();
    for value in [
        secret,
        "jira-left-out-0000",
        "unknown-value-0000",
        "ftp://jira",
    ] {
        assert!(!said.contains(value), "{value}");
    }
}

/// A `settings.json` 0.0.7 would have refused imports nothing and says why; no marker is
/// written, so a repaired file imports at the next start.
#[tokio::test(flavor = "multi_thread")]
async fn an_unreadable_settings_json_imports_nothing() {
    let fixture = Fixture::new();
    fixture.settings(serde_json::json!({ "GITLAB_URL": { "nested": URL } }));
    let (secrets, _, _) = secrets(FrontEnd::Desktop, true, &fixture.root);
    assert!(matches!(
        import_once(&fixture.place, &secrets).await,
        Some(LegacyImportReportDto::Unreadable { .. })
    ));
    assert_eq!(fixture.config(), None);
    assert!(!fixture.marker().exists());
}

/// The API server only says how to import: once while it is due, nothing once a config exists;
/// it writes nothing.
#[test]
fn the_api_hint_names_the_file_and_writes_nothing() {
    let fixture = Fixture::new();
    let legacy = fixture.settings(gitlab_settings());
    let said = |path: &Path| {
        diagnostics::entries()
            .iter()
            .filter(|entry| entry.message.contains(&path.display().to_string()))
            .filter(|entry| entry.message.contains("activity-log sources"))
            .count()
    };
    hint(&fixture.place);
    assert_eq!(said(&legacy), 1);
    assert_eq!(fixture.config(), None);

    fs::create_dir_all(fixture.place.config.parent().unwrap()).unwrap();
    fs::write(&fixture.place.config, "").unwrap();
    hint(&fixture.place);
    assert_eq!(said(&legacy), 1);
}
