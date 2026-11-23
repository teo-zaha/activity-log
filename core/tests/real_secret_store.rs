//! The OS secure store itself, run only by CI (`--features real-store-tests`): on macOS in a
//! keychain file of the tests' own (`ACTIVITY_LOG_TEST_KEYCHAIN`), on Windows and Linux on a
//! runner marked `CI=true` and `ACTIVITY_LOG_REAL_STORE_TESTS=1`, and always on an item of
//! the tests' own, never the config key. Anywhere else the store refuses to be built and the
//! tests fail saying why, so they never touch a developer's keychain.

use activity_log_core::secrets::{Created, DataKey, Interaction, Lookup, OsStore, SecretStore};

fn store() -> OsStore {
    OsStore::for_real_store_tests().unwrap_or_else(|refusal| {
        panic!(
            "The real-store tests do not run here ({refusal:?}): on macOS set \
             ACTIVITY_LOG_TEST_KEYCHAIN to a keychain file of their own, elsewhere they run on \
             CI only"
        )
    })
}

fn found(lookup: Lookup) -> DataKey {
    match lookup {
        Lookup::Found(key) => key,
        Lookup::Absent => panic!("the store holds no key"),
    }
}

/// Absent is definite, a key is stored once and read back as stored, and a second creation
/// leaves the first key in place.
#[tokio::test]
async fn the_store_reads_creates_once_and_never_overwrites() {
    let store = store();
    store.delete().await.unwrap();
    store.probe().await.unwrap();
    assert!(matches!(
        store.read(Interaction::Allowed).await.unwrap(),
        Lookup::Absent
    ));

    let key = DataKey::generate().unwrap();
    assert_eq!(store.create_if_absent(&key).await.unwrap(), Created::Stored);
    assert!(found(store.read(Interaction::Allowed).await.unwrap()).same_as(&key));

    let other = DataKey::generate().unwrap();
    assert_eq!(
        store.create_if_absent(&other).await.unwrap(),
        Created::AlreadyThere
    );
    assert!(found(store.read(Interaction::Allowed).await.unwrap()).same_as(&key));
    // The item this program created needs no dialog to be read.
    assert!(found(store.read(Interaction::Never).await.unwrap()).same_as(&key));

    store.delete().await.unwrap();
    assert!(matches!(
        store.read(Interaction::Allowed).await.unwrap(),
        Lookup::Absent
    ));
}

/// Without a session bus there is no Secret Service: `Unavailable`, never absent. CI runs it by
/// name, with the bus's address unset (`--ignored`).
#[cfg(target_os = "linux")]
#[tokio::test]
#[ignore = "run by CI without a session bus"]
async fn without_a_session_bus_the_store_is_unavailable() {
    use activity_log_core::secrets::StoreErrorKind;

    let store = store();
    for err in [
        store.read(Interaction::Allowed).await.unwrap_err(),
        store.probe().await.unwrap_err(),
    ] {
        assert_eq!(err.kind(), StoreErrorKind::Unavailable, "{err}");
    }
}
