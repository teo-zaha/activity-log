//! What every store of the config key answers: the OS store, the key file, the tests' memory.

use std::{fmt, sync::Arc};

use futures::future::BoxFuture;
use zeroize::Zeroizing;

/// The length of the config key: AES-SIV with AES-256 takes a 512-bit key.
pub const KEY_LEN: usize = 64;

/// The config key that seals every secret in `config.toml`. Its bytes live on the heap, are
/// wiped when it is dropped (best effort: copies the OS made are out of reach) and are never
/// printed.
#[derive(Clone)]
pub struct DataKey(Box<Zeroizing<[u8; KEY_LEN]>>);

impl DataKey {
    /// A new key from the OS's random number generator.
    pub fn generate() -> Result<Self, StoreError> {
        let mut key = Self::zeroed();
        getrandom::fill(key.0.as_mut_slice())
            .map_err(|err| StoreError::failed(anyhow::anyhow!("No random bytes: {err}")))?;
        Ok(key)
    }

    /// The key held in `bytes`, `None` unless they are exactly a key's length. The bytes are
    /// copied straight into the key's own buffer.
    pub(crate) fn from_slice(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != KEY_LEN {
            return None;
        }
        let mut key = Self::zeroed();
        key.0.copy_from_slice(bytes);
        Some(key)
    }

    fn zeroed() -> Self {
        Self(Box::new(Zeroizing::new([0u8; KEY_LEN])))
    }

    pub(crate) fn bytes(&self) -> &[u8; KEY_LEN] {
        &self.0
    }

    /// Whether `other` is this key, compared in constant time.
    pub fn same_as(&self, other: &DataKey) -> bool {
        use subtle::ConstantTimeEq;
        self.0.as_slice().ct_eq(other.0.as_slice()).into()
    }
}

impl fmt::Debug for DataKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DataKey(<redacted>)")
    }
}

/// Which store holds a key: the OS secure store or the key file. A sealed value names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, strum::AsRefStr, strum::EnumIter)]
#[strum(serialize_all = "lowercase")]
pub enum StoreKind {
    Os,
    File,
}

/// Whether the store may show a dialog (a keychain prompt, an unlock prompt).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interaction {
    Allowed,
    /// Answer only what needs no dialog; a store that would need one is `Refused`. On macOS it
    /// turns interaction off for the whole process while it asks, so it is only for a moment
    /// nothing else runs (the desktop's start, before its host).
    Never,
}

/// A store's answer to a read.
#[derive(Debug, Clone)]
pub enum Lookup {
    Found(DataKey),
    /// Definite: the store answered, it is usable, and it holds no key. Nothing else is
    /// absent, and only over this answer is a key ever created.
    Absent,
}

/// A store's answer to a creation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Created {
    Stored,
    /// Another writer stored a key first; it was left as it is.
    AlreadyThere,
}

/// Why a store gave no answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::AsRefStr)]
#[strum(serialize_all = "camelCase")]
pub enum StoreErrorKind {
    /// There is no such store here (no default keychain, a logon without a credential store,
    /// no Secret Service): the CLI may use the key file instead.
    Unavailable,
    /// The store is there but would not give the key now (interaction not allowed, a dialog
    /// cancelled, an unlock prompt dismissed): try again, never fall back.
    Refused,
    Failed,
}

/// A store's failure: its kind, to decide by, and its cause, for people. Never holds a key.
#[derive(Debug, Clone)]
pub struct StoreError {
    kind: StoreErrorKind,
    cause: Arc<anyhow::Error>,
}

impl StoreError {
    pub fn new(kind: StoreErrorKind, cause: anyhow::Error) -> Self {
        Self {
            kind,
            cause: Arc::new(cause),
        }
    }

    pub fn unavailable(cause: anyhow::Error) -> Self {
        Self::new(StoreErrorKind::Unavailable, cause)
    }

    pub fn refused(cause: anyhow::Error) -> Self {
        Self::new(StoreErrorKind::Refused, cause)
    }

    pub fn failed(cause: anyhow::Error) -> Self {
        Self::new(StoreErrorKind::Failed, cause)
    }

    pub fn kind(&self) -> StoreErrorKind {
        self.kind
    }
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#}", self.cause)
    }
}

impl std::error::Error for StoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.cause.as_ref().as_ref())
    }
}

/// One store of the config key. It holds at most one key; it never overwrites one and never
/// creates one over anything but a definite [`Lookup::Absent`].
pub trait SecretStore: Send + Sync + 'static {
    fn kind(&self) -> StoreKind;

    fn read(&self, interaction: Interaction) -> BoxFuture<'_, Result<Lookup, StoreError>>;

    /// Whether the store is there and answering, without reading the key and without a dialog.
    fn probe(&self) -> BoxFuture<'_, Result<(), StoreError>>;

    /// The right to create the key here; `None` for a store this program only reads (the key
    /// file of the desktop app and the API server), so no creation can even be asked of it.
    fn creator(&self) -> Option<&dyn KeyCreator>;
}

/// What creates the key in a store.
pub trait KeyCreator: Send + Sync {
    /// Stores `key` unless the store holds one. Called only under the key-creation lock (see
    /// [`Secrets`](super::Secrets)), right after a read found none: a store that cannot refuse to
    /// overwrite (Windows' credentials, the Secret Service, the key file) checks again first.
    fn create_if_absent<'a>(
        &'a self,
        key: &'a DataKey,
    ) -> BoxFuture<'a, Result<Created, StoreError>>;
}
