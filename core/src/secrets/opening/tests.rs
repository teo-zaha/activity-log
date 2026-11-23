use std::{collections::BTreeMap, sync::Arc};

use super::{Unopened, open_connection};
use crate::{
    model::{ProviderProblem, Source},
    provider::{
        connection::{ConnectionValues, StoredSecret, StoredValue},
        gitlab::GITLAB_TOKEN,
        provider_config::ConfigSource,
    },
    secrets::{
        DataKey, FrontEnd, KEY_LEN, MemoryStore, SecretStore, Secrets, StoreCall, StoreErrorKind,
        StoreKind,
        sealed::{Binding, Malformed, SealedValue, Unopenable, seal},
    },
};

const URL: &str = "https://gitlab.example.test";
const TOKEN: &str = "glpat-test-0000";

fn key(byte: u8) -> DataKey {
    DataKey::from_slice(&[byte; KEY_LEN]).unwrap()
}

/// Stores in memory: the OS store holding `os_key`, an empty key file.
fn secrets(os_key: Option<DataKey>) -> (Secrets, Arc<MemoryStore>) {
    let os = Arc::new(match os_key {
        Some(key) => MemoryStore::holding(StoreKind::Os, key),
        None => MemoryStore::new(StoreKind::Os),
    });
    let file: Arc<dyn SecretStore> = Arc::new(MemoryStore::new(StoreKind::File));
    let secrets = Secrets::with_stores(
        FrontEnd::Cli,
        Some((os.clone() as Arc<dyn SecretStore>, None)),
        (file, None),
    );
    (secrets, os)
}

fn plain(url: &str) -> BTreeMap<String, Option<StoredValue>> {
    [
        ("GITLAB_URL", StoredValue::Text(url.to_string())),
        ("GITLAB_USER", StoredValue::Text("jane".to_string())),
    ]
    .into_iter()
    .map(|(key, value)| (key.to_string(), Some(value)))
    .collect()
}

/// GitLab values with the token sealed by `sealed_with` for `sealed_for`, the file now
/// naming `url`.
fn gitlab(url: &str, sealed_with: &DataKey, sealed_for: &str) -> ConnectionValues {
    let binding = Binding::of(
        &Source::Gitlab,
        &ConnectionValues::stored(plain(sealed_for)),
    )
    .unwrap();
    let sealed = seal(sealed_with, StoreKind::Os, GITLAB_TOKEN, &binding, TOKEN).unwrap();
    with_token(url, StoredSecret::Sealed(sealed))
}

fn with_token(url: &str, token: StoredSecret) -> ConnectionValues {
    let mut values = plain(url);
    values.insert("GITLAB_TOKEN".to_string(), Some(StoredValue::Sealed(token)));
    ConnectionValues::stored(values)
}

async fn opened(values: &ConnectionValues, secrets: &Secrets) -> Result<String, Unopened> {
    let opened = open_connection(&Source::Gitlab, values, secrets).await?;
    Ok(opened.over(values).get("GITLAB_TOKEN").unwrap())
}

/// A sealed token opens with the store's key and is read in its place; the values alone never
/// hand on a sealed value's text.
#[tokio::test]
async fn a_sealed_token_opens_with_the_stores_key() {
    let (secrets, os) = secrets(Some(key(1)));
    let values = gitlab(URL, &key(1), URL);
    assert_eq!(values.get("GITLAB_TOKEN"), None);
    assert_eq!(opened(&values, &secrets).await.unwrap(), TOKEN);
    // Asked once; the key is held for every later connection.
    assert_eq!(opened(&values, &secrets).await.unwrap(), TOKEN);
    assert_eq!(os.calls().reads, 1);
}

#[tokio::test]
async fn plain_values_need_no_store() {
    let (secrets, os) = secrets(None);
    let mut values = plain(URL);
    values.insert(
        "GITLAB_TOKEN".to_string(),
        Some(StoredValue::Text(TOKEN.to_string())),
    );
    let values = ConnectionValues::stored(values);
    assert_eq!(opened(&values, &secrets).await.unwrap(), TOKEN);
    assert_eq!(os.calls().reads, 0);
}

/// Each way a token does not open is told apart, and only the store's own trouble is
/// `secretUnavailable`; the rest are values to enter again.
#[tokio::test]
async fn each_reason_a_token_does_not_open_has_its_problem() {
    let (no_key, _) = secrets(None);
    let err = opened(&gitlab(URL, &key(1), URL), &no_key)
        .await
        .unwrap_err();
    assert!(matches!(err, Unopened::NoKey { .. }), "{err:?}");
    assert_eq!(err.problem(), ProviderProblem::NotConfigured);

    let (other_key, _) = secrets(Some(key(2)));
    let err = opened(&gitlab(URL, &key(1), URL), &other_key)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        Unopened::Unopenable {
            why: Unopenable::OtherKey,
            ..
        }
    ));

    let (right_key, _) = secrets(Some(key(1)));
    let moved = gitlab("https://evil.example.test", &key(1), URL);
    let err = opened(&moved, &right_key).await.unwrap_err();
    assert!(matches!(
        err,
        Unopened::Unopenable {
            why: Unopenable::OtherAddress,
            ..
        }
    ));
    assert_eq!(err.problem(), ProviderProblem::NotConfigured);

    let unbound = gitlab("https://jane:pw@gitlab.example.test", &key(1), URL);
    let err = opened(&unbound, &right_key).await.unwrap_err();
    assert!(matches!(err, Unopened::Unbound { .. }), "{err:?}");

    for kind in [
        StoreErrorKind::Unavailable,
        StoreErrorKind::Refused,
        StoreErrorKind::Failed,
    ] {
        let (refusing, os) = secrets(Some(key(1)));
        os.fail_next(StoreCall::Read, kind);
        let err = opened(&gitlab(URL, &key(1), URL), &refusing)
            .await
            .unwrap_err();
        assert!(matches!(&err, Unopened::Store { error, .. } if error.kind() == kind));
        assert_eq!(err.problem(), ProviderProblem::SecretUnavailable);
    }
}

/// A sealed text that does not read is its own case, never a damaged value: one from a newer
/// version says so.
#[tokio::test]
async fn an_unreadable_sealed_text_says_why() {
    let (secrets, os) = secrets(Some(key(1)));
    for (text, malformed) in [
        (
            "v2:os:00000000:00000000:AAAAAAAAAAAAAAAAAAAAAA",
            Malformed::Version,
        ),
        (
            "v1:os:XYZ:00000000:AAAAAAAAAAAAAAAAAAAAAA",
            Malformed::KeyId,
        ),
    ] {
        assert_eq!(SealedValue::parse(text).err(), Some(malformed));
        let values = with_token(
            URL,
            StoredSecret::Unreadable {
                text: text.to_string(),
                malformed,
            },
        );
        let err = opened(&values, &secrets).await.unwrap_err();
        assert!(
            matches!(err, Unopened::Unreadable { why, .. } if why == malformed),
            "{err:?}"
        );
        assert_eq!(err.problem(), ProviderProblem::NotConfigured);
    }
    assert_eq!(os.calls().reads, 0);
}
