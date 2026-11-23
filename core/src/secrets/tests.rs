use std::{cell::Cell, path::PathBuf, sync::Arc, time::Duration};

use tempfile::{TempDir, tempdir};

use super::{
    DataKey, FreshKey, Interaction, KEY_WAIT, MemoryStore, SecretStore, Secrets, StoreCall,
    StoreCalls, StoreErrorKind, StoreKind, take_creation_lock,
};

/// `Secrets` over an OS store in memory, with its key-creation lock in a folder of its own.
struct Fixture {
    os: Arc<MemoryStore>,
    secrets: Secrets,
    lock: PathBuf,
    _dir: TempDir,
}

fn fixture(os: MemoryStore) -> Fixture {
    let dir = tempdir().unwrap();
    let lock = dir.path().join("activity-log").join("secret-key.lock");
    let os = Arc::new(os);
    let file: Arc<dyn SecretStore> = Arc::new(MemoryStore::new(StoreKind::File));
    let secrets = Secrets::with_stores(
        Some((os.clone() as Arc<dyn SecretStore>, Some(lock.clone()))),
        (file, None),
    );
    Fixture {
        os,
        secrets,
        lock,
        _dir: dir,
    }
}

fn key() -> DataKey {
    DataKey::generate().unwrap()
}

fn reads(store: &MemoryStore) -> usize {
    store.calls().reads
}

/// Readers that ask together share one read of the store, so one dialog shows.
#[tokio::test]
async fn concurrent_first_readers_share_one_read() {
    let stored = key();
    let f = fixture(MemoryStore::holding(StoreKind::Os, stored.clone()));
    let gate = f.os.hold_reads();
    let readers: Vec<_> = (0..4)
        .map(|_| {
            let secrets = f.secrets.clone();
            tokio::spawn(async move { secrets.key(StoreKind::Os).await })
        })
        .collect();
    tokio::task::yield_now().await;
    gate.release();
    for reader in readers {
        let found = reader.await.unwrap().unwrap().unwrap();
        assert!(found.same_as(&stored));
    }
    assert_eq!(reads(&f.os), 1);
    assert_eq!(f.os.read_interactions(), [Interaction::Allowed]);

    // The key is held: the next reader does not ask the store.
    f.secrets.key(StoreKind::Os).await.unwrap().unwrap();
    assert_eq!(reads(&f.os), 1);
    assert!(f.secrets.held(StoreKind::Os).unwrap().same_as(&stored));
}

/// A refusal or a failure is never kept, and neither is an absent key: the next reader asks.
#[tokio::test]
async fn only_a_found_key_is_held() {
    let f = fixture(MemoryStore::new(StoreKind::Os));
    for kind in [
        StoreErrorKind::Refused,
        StoreErrorKind::Unavailable,
        StoreErrorKind::Failed,
    ] {
        f.os.fail_next(StoreCall::Read, kind);
        assert_eq!(f.secrets.key(StoreKind::Os).await.unwrap_err().kind(), kind);
        assert!(f.secrets.held(StoreKind::Os).is_none());
    }
    assert!(f.secrets.key(StoreKind::Os).await.unwrap().is_none());
    assert!(f.secrets.held(StoreKind::Os).is_none());

    let stored = key();
    f.os.set_key(Some(stored.clone()));
    let found = f.secrets.key(StoreKind::Os).await.unwrap().unwrap();
    assert!(found.same_as(&stored));
    assert_eq!(reads(&f.os), 5);
}

/// A reader waits at most `KEY_WAIT`; the read goes on and its key is held for the next one.
#[tokio::test(start_paused = true)]
async fn a_reader_stops_waiting_but_the_answer_is_kept() {
    let stored = key();
    let f = fixture(MemoryStore::holding(StoreKind::Os, stored.clone()));
    let gate = f.os.hold_reads();
    let started = tokio::time::Instant::now();
    let err = f.secrets.key(StoreKind::Os).await.unwrap_err();
    assert_eq!(err.kind(), StoreErrorKind::Failed);
    assert!(started.elapsed() >= KEY_WAIT);

    gate.release();
    while f.secrets.held(StoreKind::Os).is_none() {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        f.secrets
            .key(StoreKind::Os)
            .await
            .unwrap()
            .unwrap()
            .same_as(&stored)
    );
    assert_eq!(reads(&f.os), 1);
}

/// A writer reads the store again, whatever is held, so a replaced key is noticed.
#[tokio::test]
async fn a_writer_reads_the_key_again() {
    let f = fixture(MemoryStore::holding(StoreKind::Os, key()));
    f.secrets.key(StoreKind::Os).await.unwrap().unwrap();
    let replaced = key();
    f.os.set_key(Some(replaced.clone()));

    let FreshKey::Found(fresh) = f.secrets.fresh_key(StoreKind::Os).await.unwrap() else {
        panic!("the store holds a key");
    };
    assert!(fresh.same_as(&replaced));
    assert_eq!(reads(&f.os), 2);
    assert!(f.secrets.held(StoreKind::Os).unwrap().same_as(&replaced));
}

/// Nothing is created over a store that is missing, refusing or failing.
#[tokio::test]
async fn no_key_is_created_unless_the_store_said_absent() {
    let f = fixture(MemoryStore::new(StoreKind::Os));
    for kind in [
        StoreErrorKind::Unavailable,
        StoreErrorKind::Refused,
        StoreErrorKind::Failed,
    ] {
        f.os.fail_next(StoreCall::Read, kind);
        let err = f.secrets.fresh_key(StoreKind::Os).await.err().unwrap();
        assert_eq!(err.kind(), kind);

        // Absent first, then a failure on the read under the lock.
        let FreshKey::Absent(creation) = f.secrets.fresh_key(StoreKind::Os).await.unwrap() else {
            panic!("the store holds no key");
        };
        f.os.fail_next(StoreCall::Read, kind);
        let warned = Cell::new(false);
        let err = creation.create(|| warned.set(true)).await.unwrap_err();
        assert_eq!(err.kind(), kind);
        assert!(!warned.get());
    }
    assert_eq!(f.os.calls().creates, 0);
    assert!(f.os.key().is_none());
}

/// Created under its lock, after the warning, and read back before it is used.
#[tokio::test]
async fn a_key_is_created_once_warned_and_read_back() {
    let f = fixture(MemoryStore::new(StoreKind::Os));
    let FreshKey::Absent(creation) = f.secrets.fresh_key(StoreKind::Os).await.unwrap() else {
        panic!("the store holds no key");
    };
    let warnings = Cell::new(0);
    let created = creation
        .create(|| warnings.set(warnings.get() + 1))
        .await
        .unwrap();
    assert_eq!(warnings.get(), 1);
    assert!(created.same_as(&f.os.key().unwrap()));
    assert!(f.secrets.held(StoreKind::Os).unwrap().same_as(&created));
    assert_eq!(
        f.os.calls(),
        StoreCalls {
            reads: 3,
            creates: 1,
            probes: 0
        }
    );
    assert!(f.lock.exists());
}

/// A key another writer stored before the lock was taken is used, with no warning and no
/// creation.
#[tokio::test]
async fn a_key_stored_meanwhile_is_used() {
    let f = fixture(MemoryStore::new(StoreKind::Os));
    let FreshKey::Absent(creation) = f.secrets.fresh_key(StoreKind::Os).await.unwrap() else {
        panic!("the store holds no key");
    };
    let theirs = key();
    f.os.set_key(Some(theirs.clone()));
    let warned = Cell::new(false);
    let used = creation.create(|| warned.set(true)).await.unwrap();
    assert!(used.same_as(&theirs));
    assert!(!warned.get());
    assert_eq!(f.os.calls().creates, 0);
}

/// A key that does not read back as generated is not used.
#[tokio::test]
async fn a_key_that_reads_back_otherwise_is_refused() {
    let f = fixture(MemoryStore::new(StoreKind::Os));
    f.os.create_instead(key());
    let FreshKey::Absent(creation) = f.secrets.fresh_key(StoreKind::Os).await.unwrap() else {
        panic!("the store holds no key");
    };
    let err = creation.create(|| ()).await.unwrap_err();
    assert_eq!(err.kind(), StoreErrorKind::Failed);
    assert!(f.secrets.held(StoreKind::Os).is_none());
}

#[tokio::test]
async fn without_a_lock_folder_no_key_is_created() {
    let os = Arc::new(MemoryStore::new(StoreKind::Os));
    let file: Arc<dyn SecretStore> = Arc::new(MemoryStore::new(StoreKind::File));
    let secrets = Secrets::with_stores(
        Some((os.clone() as Arc<dyn SecretStore>, None)),
        (file, None),
    );
    let FreshKey::Absent(creation) = secrets.fresh_key(StoreKind::Os).await.unwrap() else {
        panic!("the store holds no key");
    };
    assert_eq!(
        creation.create(|| ()).await.unwrap_err().kind(),
        StoreErrorKind::Failed
    );
    assert_eq!(os.calls().creates, 0);
}

#[tokio::test]
async fn without_an_os_store_its_key_is_unavailable() {
    let file: Arc<dyn SecretStore> = Arc::new(MemoryStore::new(StoreKind::File));
    let secrets = Secrets::with_stores(None, (file, None));
    assert_eq!(
        secrets.key(StoreKind::Os).await.unwrap_err().kind(),
        StoreErrorKind::Unavailable
    );
    assert!(secrets.key(StoreKind::File).await.unwrap().is_none());
}

/// A creation lock another process holds is a refusal to try again, after the wait.
#[test]
fn a_held_creation_lock_refuses_after_its_wait() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("activity-log").join("secret-key.lock");
    let held = take_creation_lock(&path, Duration::ZERO).unwrap();
    let err = take_creation_lock(&path, Duration::from_millis(100)).unwrap_err();
    assert_eq!(err.kind(), StoreErrorKind::Refused);
    drop(held);
    take_creation_lock(&path, Duration::ZERO).unwrap();
}

#[test]
fn a_key_never_prints() {
    let key = key();
    assert_eq!(format!("{key:?}"), "DataKey(<redacted>)");
    assert!(key.same_as(&key.clone()));
    assert!(!key.same_as(&DataKey::generate().unwrap()));
}

/// A read task that panics leaves nothing held and no attempt behind: the next reader asks.
#[tokio::test]
async fn a_panicked_read_leaves_the_slot_free() {
    let stored = key();
    let f = fixture(MemoryStore::holding(StoreKind::Os, stored.clone()));
    f.os.panic_next_read();
    assert_eq!(
        f.secrets.key(StoreKind::Os).await.unwrap_err().kind(),
        StoreErrorKind::Failed
    );
    let found = f.secrets.key(StoreKind::Os).await.unwrap().unwrap();
    assert!(found.same_as(&stored));
    assert_eq!(reads(&f.os), 2);
}

/// An attempt that never ends holds no one: a reader that comes once it is `KEY_WAIT` old
/// starts another.
#[tokio::test(start_paused = true)]
async fn an_attempt_older_than_the_wait_is_asked_again() {
    let stored = key();
    let f = fixture(MemoryStore::holding(StoreKind::Os, stored.clone()));
    let gate = f.os.hold_reads();
    f.secrets.key(StoreKind::Os).await.unwrap_err();
    let second = {
        let secrets = f.secrets.clone();
        tokio::spawn(async move { secrets.key(StoreKind::Os).await })
    };
    tokio::time::sleep(Duration::from_millis(10)).await;
    assert_eq!(reads(&f.os), 2);
    gate.release();
    assert!(second.await.unwrap().unwrap().unwrap().same_as(&stored));
}

/// A store this program only reads gives no creation right: no lock, no warning, no key.
#[tokio::test]
async fn a_read_only_store_offers_no_creation() {
    let dir = tempdir().unwrap();
    let lock = dir.path().join("activity-log").join("secret.key.lock");
    let file = Arc::new(MemoryStore::new(StoreKind::File).read_only());
    let secrets = Secrets::with_stores(
        None,
        (file.clone() as Arc<dyn SecretStore>, Some(lock.clone())),
    );
    assert!(matches!(
        secrets.fresh_key(StoreKind::File).await.unwrap(),
        FreshKey::AbsentReadOnly
    ));
    assert_eq!(file.calls().creates, 0);
    assert!(!lock.parent().unwrap().exists());
}
