use std::{fs, path::Path};

use tempfile::tempdir;

use super::{KeyFileAccess, KeyFileStore};
use crate::secrets::{
    Created, DataKey, Interaction, Lookup, SecretStore, StoreErrorKind, StoreKind,
};

fn store_at(path: &Path, access: KeyFileAccess) -> KeyFileStore {
    KeyFileStore::new(Some(path.to_path_buf()), access)
}

async fn create(
    store: &KeyFileStore,
    key: &DataKey,
) -> Result<Created, crate::secrets::StoreError> {
    let creator = store
        .creator()
        .expect("a store that may create the key file");
    creator.create_if_absent(key, Interaction::Allowed).await
}

async fn read(store: &KeyFileStore) -> Result<Lookup, crate::secrets::StoreError> {
    store.read(Interaction::Allowed).await
}

#[tokio::test]
async fn a_created_key_reads_back_and_is_never_replaced() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("activity-log").join("secret.key");
    let store = store_at(&path, KeyFileAccess::Create);
    assert_eq!(store.kind(), StoreKind::File);
    assert!(matches!(read(&store).await.unwrap(), Lookup::Absent));

    let key = DataKey::generate().unwrap();
    assert_eq!(create(&store, &key).await.unwrap(), Created::Stored);
    let Lookup::Found(stored) = read(&store).await.unwrap() else {
        panic!("no key after creating it");
    };
    assert!(stored.same_as(&key));
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.starts_with("activity-log config key v1\n"), "{text}");
    assert_eq!(text.len(), "activity-log config key v1\n".len() + 129);

    let other = DataKey::generate().unwrap();
    assert_eq!(create(&store, &other).await.unwrap(), Created::AlreadyThere);
    assert_eq!(fs::read_to_string(&path).unwrap(), text);
}

/// The desktop app and the API server only read the key file: there is no creation to ask.
#[tokio::test]
async fn a_read_only_key_file_is_never_created() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("secret.key");
    let store = store_at(&path, KeyFileAccess::ReadOnly);
    assert!(store.creator().is_none());
    assert!(matches!(read(&store).await.unwrap(), Lookup::Absent));
    assert!(!path.exists());
}

/// A file that holds no key is a failed store, never an absent key: it is not replaced.
#[tokio::test]
async fn a_damaged_key_file_is_not_absent_and_stays() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("secret.key");
    let store = store_at(&path, KeyFileAccess::Create);
    create(&store, &DataKey::generate().unwrap()).await.unwrap();
    for damaged in [
        "activity-log config key v1\nabcd\n".to_string(),
        "another format\n".to_string(),
        format!("activity-log config key v1\n{}\n", "zz".repeat(64)),
    ] {
        fs::write(&path, &damaged).unwrap();
        restrict(&path);
        assert_eq!(
            read(&store).await.unwrap_err().kind(),
            StoreErrorKind::Failed
        );
        assert_eq!(
            create(&store, &DataKey::generate().unwrap()).await.unwrap(),
            Created::AlreadyThere
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), damaged);
    }
}

#[tokio::test]
async fn without_a_location_there_is_no_key_file() {
    let store = KeyFileStore::new(None, KeyFileAccess::Create);
    assert_eq!(
        read(&store).await.unwrap_err().kind(),
        StoreErrorKind::Unavailable
    );
    assert_eq!(
        store.probe().await.unwrap_err().kind(),
        StoreErrorKind::Unavailable
    );
    assert_eq!(store.creation_lock(), None);
}

#[test]
fn the_creation_lock_is_beside_the_key_file() {
    let store = KeyFileStore::new(
        Some(Path::new("/data/activity-log/secret.key").to_path_buf()),
        KeyFileAccess::Create,
    );
    assert_eq!(
        store.creation_lock().unwrap(),
        Path::new("/data/activity-log/secret.key.lock")
    );
}

/// A key other accounts can read, or one in a folder they can change, is not used.
#[cfg(unix)]
#[tokio::test]
async fn a_key_file_others_can_reach_is_not_used() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().unwrap();
    let folder = dir.path().join("activity-log");
    let path = folder.join("secret.key");
    let store = store_at(&path, KeyFileAccess::Create);
    create(&store, &DataKey::generate().unwrap()).await.unwrap();
    assert!(matches!(read(&store).await.unwrap(), Lookup::Found(_)));

    // Readable, or only writable, by the group or others: either way not the user's alone.
    for mode in [0o644, 0o640, 0o620, 0o602] {
        fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
        assert_eq!(
            read(&store).await.unwrap_err().kind(),
            StoreErrorKind::Failed,
            "mode {mode:o}"
        );
    }
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    assert!(matches!(read(&store).await.unwrap(), Lookup::Found(_)));

    fs::set_permissions(&folder, fs::Permissions::from_mode(0o777)).unwrap();
    assert_eq!(
        read(&store).await.unwrap_err().kind(),
        StoreErrorKind::Failed
    );
}

#[cfg(unix)]
fn restrict(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}

#[cfg(not(unix))]
fn restrict(_path: &Path) {}
