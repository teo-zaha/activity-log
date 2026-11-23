//! The OS secure store as a [`SecretStore`]. Only the binaries build the user's (`Secrets`'
//! front-end constructors, which core's own tests cannot reach); the real-store tests build one
//! of their own, which refuses to exist outside CI or a keychain of its own.

use futures::{FutureExt, future::BoxFuture};

use super::{Created, DataKey, Interaction, Lookup, SecretStore, StoreError, StoreKind};
use crate::platform::secret_store::{self, Place};

pub struct OsStore {
    place: Place,
}

impl OsStore {
    /// The config key in the user's own store.
    #[cfg(not(test))]
    pub(crate) fn user() -> Self {
        Self {
            place: Place::config_key(),
        }
    }

    /// The real-store tests' own item, never the config key: on macOS in the keychain file
    /// `ACTIVITY_LOG_TEST_KEYCHAIN` names (never the login keychain), elsewhere only on a CI
    /// runner (`CI=true` and `ACTIVITY_LOG_REAL_STORE_TESTS=1`).
    #[cfg(feature = "real-store-tests")]
    pub fn for_real_store_tests() -> Result<Self, RealStoreRefusal> {
        use crate::platform::secret_store::ItemName;

        let keychain = if cfg!(target_os = "macos") {
            let path = crate::platform::dirs::env_path("ACTIVITY_LOG_TEST_KEYCHAIN")
                .ok_or(RealStoreRefusal::NoTestKeychain)?;
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            if !path.is_absolute() || name.starts_with("login.") {
                return Err(RealStoreRefusal::NoTestKeychain);
            }
            Some(path)
        } else {
            let marked = std::env::var("CI").as_deref() == Ok("true")
                && std::env::var("ACTIVITY_LOG_REAL_STORE_TESTS").as_deref() == Ok("1");
            if !marked {
                return Err(RealStoreRefusal::NotCi);
            }
            None
        };
        Ok(Self {
            place: Place {
                name: ItemName::Test,
                keychain,
            },
        })
    }

    /// Removes the tests' item.
    #[cfg(feature = "real-store-tests")]
    pub async fn delete(&self) -> Result<(), StoreError> {
        secret_store::delete(&self.place).await
    }
}

/// Why the real-store tests may not run here.
#[cfg(feature = "real-store-tests")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RealStoreRefusal {
    /// macOS: `ACTIVITY_LOG_TEST_KEYCHAIN` must name a keychain file of the tests' own, by an
    /// absolute path, never the login keychain.
    NoTestKeychain,
    /// Windows and Linux: only a CI runner, marked by `CI=true` and
    /// `ACTIVITY_LOG_REAL_STORE_TESTS=1`, runs them.
    NotCi,
}

impl SecretStore for OsStore {
    fn kind(&self) -> StoreKind {
        StoreKind::Os
    }

    fn read(&self, interaction: Interaction) -> BoxFuture<'_, Result<Lookup, StoreError>> {
        secret_store::read(&self.place, interaction).boxed()
    }

    fn create_if_absent<'a>(
        &'a self,
        key: &'a DataKey,
    ) -> BoxFuture<'a, Result<Created, StoreError>> {
        secret_store::create_if_absent(&self.place, key).boxed()
    }

    fn probe(&self) -> BoxFuture<'_, Result<(), StoreError>> {
        secret_store::probe(&self.place).boxed()
    }
}
