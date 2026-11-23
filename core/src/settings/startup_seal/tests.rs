use std::{fs, sync::Arc};

use super::{StartupSeal, seal_without_asking};
use crate::secrets::{
    FrontEnd, Interaction, MemoryStore, SecretStore, Secrets, StoreCall, StoreCalls,
    StoreErrorKind, StoreKind,
};

const PLAIN: &str = "[[gitlab]]\nname = \"work\"\nlabel = \"Work\"\nurl = \"https://gitlab.example.test\"\ntoken = \"glpat-test-0000\"\n";

fn desktop(dir: &tempfile::TempDir) -> (Secrets, Arc<MemoryStore>) {
    let os = Arc::new(MemoryStore::new(StoreKind::Os));
    let file: Arc<dyn SecretStore> = Arc::new(MemoryStore::new(StoreKind::File));
    let secrets = Secrets::with_stores(
        FrontEnd::Desktop,
        Some((
            os.clone() as Arc<dyn SecretStore>,
            Some(dir.path().join("secret-key.lock")),
        )),
        (file, None),
    );
    (secrets, os)
}

/// Plain secrets are sealed at start, the store asked without a dialog only.
#[tokio::test(flavor = "multi_thread")]
async fn plain_secrets_are_sealed_without_a_dialog() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, PLAIN).unwrap();
    let (secrets, os) = desktop(&dir);

    assert_eq!(
        seal_without_asking(&path, &secrets).await,
        StartupSeal::Sealed
    );
    let text = fs::read_to_string(&path).unwrap();
    assert!(!text.contains("glpat-test-0000"), "{text}");
    assert!(text.contains("token = { sealed = \"v1:os:"), "{text}");
    assert!(!os.read_interactions().is_empty());
    assert!(
        os.read_interactions()
            .iter()
            .all(|interaction| *interaction == Interaction::Never)
    );
}

/// A file with nothing plain asks no store and writes nothing.
#[tokio::test(flavor = "multi_thread")]
async fn nothing_plain_asks_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let sealed_or_none = "[[gitlab]]\nname = \"work\"\nlabel = \"Work\"\n";
    fs::write(&path, sealed_or_none).unwrap();
    let (secrets, os) = desktop(&dir);
    assert_eq!(
        seal_without_asking(&path, &secrets).await,
        StartupSeal::NothingToSeal
    );
    assert_eq!(os.calls(), StoreCalls::default());
    assert_eq!(fs::read_to_string(&path).unwrap(), sealed_or_none);
}

/// A store that would need a dialog, or gives no key, leaves the file as it was: the app
/// starts all the same.
#[tokio::test(flavor = "multi_thread")]
async fn a_store_that_gives_no_key_leaves_the_plain_text() {
    for kind in [
        StoreErrorKind::Refused,
        StoreErrorKind::Unavailable,
        StoreErrorKind::Failed,
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, PLAIN).unwrap();
        let (secrets, os) = desktop(&dir);
        os.fail_next(StoreCall::Read, kind);
        assert_eq!(
            seal_without_asking(&path, &secrets).await,
            StartupSeal::Skipped
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), PLAIN);
    }
}
