//! The config key that seals the secrets in `config.toml`, and where it lives: the OS secure
//! store (`platform::secret_store`) or the key file ([`KeyFileStore`]).
//!
//! [`Secrets`] is the handle each binary builds once and passes down. It holds the stores and
//! the key once read: concurrent first readers share one attempt, so one dialog shows, and a
//! failure is never kept. Writers read the key again from the store before every sealing edit
//! ([`Secrets::fresh_key`]) and create one only over a definite absence, under the
//! key-creation lock, reading it back before it is used.

use std::{
    fs::{File, TryLockError},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use anyhow::anyhow;
use futures::{
    FutureExt,
    future::{BoxFuture, Shared},
};

mod key_file;
#[cfg(any(test, feature = "test-support"))]
mod memory;
mod os_store;
pub mod sealed;
mod store;
#[cfg(test)]
mod tests;

pub use key_file::{KEY_FILE_VARIABLE, KeyFileAccess, KeyFileStore};
#[cfg(any(test, feature = "test-support"))]
pub use memory::{MemoryStore, ReadGate, StoreCall, StoreCalls};
pub use os_store::OsStore;
#[cfg(feature = "real-store-tests")]
pub use os_store::RealStoreRefusal;
pub use store::{
    Created, DataKey, Interaction, KEY_LEN, KeyCreator, Lookup, SecretStore, StoreError,
    StoreErrorKind, StoreKind,
};

/// How long a reader waits for the store (a dialog the user has not answered). The attempt
/// goes on after that: its key, once it comes, is held for the next reader; a reader that comes
/// once an attempt is this old starts another (a prompt that never completes holds no one).
pub const KEY_WAIT: Duration = Duration::from_secs(120);

/// The name of the key-creation lock for the OS store's key.
const OS_KEY_LOCK: &str = "secret-key.lock";

/// The stores of one process and the keys read from them. Cheap to clone; clones share both.
#[derive(Clone)]
pub struct Secrets(Arc<Inner>);

struct Inner {
    /// `None` only in tests that give no OS store.
    os: Option<Arc<Slot>>,
    file: Arc<Slot>,
}

/// One store, its key-creation lock and what was read from it.
struct Slot {
    store: Arc<dyn SecretStore>,
    /// `None` where there is no folder for it: no key can be created there.
    creation_lock: Option<PathBuf>,
    held: Mutex<Held>,
}

enum Held {
    Nothing,
    Asking {
        attempt: u64,
        started: tokio::time::Instant,
        answer: Shared<BoxFuture<'static, Result<Lookup, StoreError>>>,
    },
    Key(DataKey),
}

/// What a writer read from the store: the key, or none yet, which it may create only where
/// this program creates keys.
pub enum FreshKey<'a> {
    Found(DataKey),
    Absent(KeyCreation<'a>),
    /// No key, in a store this program only reads (the desktop's and the API's key file): no
    /// creation, no lock, no warning.
    AbsentReadOnly,
}

/// The right to create the key a fresh read did not find (see [`KeyCreation::create`]).
pub struct KeyCreation<'a> {
    slot: &'a Arc<Slot>,
    creator: &'a dyn KeyCreator,
}

impl Secrets {
    /// The desktop app's: the OS store, and the key file read only. With the dev profile, as
    /// for every front end, only the profile's own key file, which it creates.
    #[cfg(not(test))]
    pub fn desktop(config_path: Option<&Path>) -> Self {
        Self::of_user(config_path, KeyFileAccess::ReadOnly)
    }

    /// The CLI's: the OS store, and a key file it may create.
    #[cfg(not(test))]
    pub fn cli(config_path: Option<&Path>) -> Self {
        Self::of_user(config_path, KeyFileAccess::Create)
    }

    /// The API server's: it never writes the config, so it never creates a key.
    #[cfg(not(test))]
    pub fn api(config_path: Option<&Path>) -> Self {
        Self::of_user(config_path, KeyFileAccess::ReadOnly)
    }

    #[cfg(not(test))]
    fn of_user(config_path: Option<&Path>, access: KeyFileAccess) -> Self {
        let (os, access) = match crate::profile::current() {
            // The keychain is never asked: a rebuild never prompts, nothing is sealed into it.
            crate::profile::Profile::Dev => (None, KeyFileAccess::Create),
            crate::profile::Profile::Installed => (
                Some((
                    Arc::new(OsStore::user()) as Arc<dyn SecretStore>,
                    os_key_lock(config_path),
                )),
                access,
            ),
        };
        let file = KeyFileStore::new(KeyFileStore::default_path(), access);
        let file_lock = file.creation_lock();
        Self::build(os, (Arc::new(file), file_lock))
    }

    /// Any stores, each with its key-creation lock.
    #[cfg(any(test, feature = "test-support"))]
    pub fn with_stores(
        os: Option<(Arc<dyn SecretStore>, Option<PathBuf>)>,
        file: (Arc<dyn SecretStore>, Option<PathBuf>),
    ) -> Self {
        Self::build(os, file)
    }

    fn build(
        os: Option<(Arc<dyn SecretStore>, Option<PathBuf>)>,
        (file, file_lock): (Arc<dyn SecretStore>, Option<PathBuf>),
    ) -> Self {
        let slot = |store, creation_lock| {
            Arc::new(Slot {
                store,
                creation_lock,
                held: Mutex::new(Held::Nothing),
            })
        };
        Self(Arc::new(Inner {
            os: os.map(|(store, lock)| slot(store, lock)),
            file: slot(file, file_lock),
        }))
    }

    /// A reader's key in the store of `kind`: the one held, else what one read of the store
    /// finds for every reader asking meanwhile, waiting at most [`KEY_WAIT`]. `None` when the
    /// store holds no key.
    pub async fn key(&self, kind: StoreKind) -> Result<Option<DataKey>, StoreError> {
        let slot = self.slot(kind)?;
        let answer = {
            let mut held = slot.held();
            match &*held {
                Held::Key(key) => return Ok(Some(key.clone())),
                Held::Asking {
                    answer, started, ..
                } if started.elapsed() < KEY_WAIT => answer.clone(),
                Held::Asking { .. } | Held::Nothing => {
                    let (attempt, answer) = slot.ask();
                    *held = Held::Asking {
                        attempt,
                        started: tokio::time::Instant::now(),
                        answer: answer.clone(),
                    };
                    answer
                }
            }
        };
        match tokio::time::timeout(KEY_WAIT, answer).await {
            Ok(Ok(Lookup::Found(key))) => Ok(Some(key)),
            Ok(Ok(Lookup::Absent)) => Ok(None),
            Ok(Err(err)) => Err(err),
            Err(_) => Err(StoreError::failed(anyhow!(
                "The secure store did not answer within {} s",
                KEY_WAIT.as_secs()
            ))),
        }
    }

    /// The key of `kind` already held, without asking the store.
    pub fn held(&self, kind: StoreKind) -> Option<DataKey> {
        let slot = self.slot(kind).ok()?;
        match &*slot.held() {
            Held::Key(key) => Some(key.clone()),
            Held::Nothing | Held::Asking { .. } => None,
        }
    }

    /// A writer's key: read again from the store, a dialog allowed, so a key another process
    /// deleted or replaced is noticed before anything is sealed with it. What it finds is held
    /// for readers too.
    pub async fn fresh_key(&self, kind: StoreKind) -> Result<FreshKey<'_>, StoreError> {
        let slot = self.slot_ref(kind)?;
        match slot.store.read(Interaction::Allowed).await? {
            Lookup::Found(key) => {
                slot.hold(&key);
                Ok(FreshKey::Found(key))
            }
            Lookup::Absent => Ok(match slot.store.creator() {
                Some(creator) => FreshKey::Absent(KeyCreation { slot, creator }),
                None => FreshKey::AbsentReadOnly,
            }),
        }
    }

    fn slot(&self, kind: StoreKind) -> Result<Arc<Slot>, StoreError> {
        self.slot_ref(kind).cloned()
    }

    fn slot_ref(&self, kind: StoreKind) -> Result<&Arc<Slot>, StoreError> {
        match kind {
            StoreKind::Os => self.0.os.as_ref().ok_or_else(|| {
                StoreError::unavailable(anyhow!("This program has no OS secure store"))
            }),
            StoreKind::File => Ok(&self.0.file),
        }
    }
}

impl KeyCreation<'_> {
    /// Creates the key under the key-creation lock: the store is read once more, and only if
    /// it still holds none is `before_creating` called (the writer's warning about values
    /// sealed with a key this computer does not have) and a new key stored, then read back
    /// through a fresh lookup and compared with what was generated. Another writer's key,
    /// found on the way, is used instead.
    pub async fn create(self, before_creating: impl FnOnce()) -> Result<DataKey, StoreError> {
        let slot = self.slot;
        let lock = slot.creation_lock.clone().ok_or_else(|| {
            StoreError::failed(anyhow!("There is no folder for the key-creation lock"))
        })?;
        let _lock = tokio::task::spawn_blocking(move || {
            take_creation_lock(&lock, crate::settings::lock_wait())
        })
        .await
        .unwrap_or_else(|err| Err(StoreError::failed(anyhow!("Locking stopped: {err}"))))?;
        if let Lookup::Found(key) = slot.store.read(Interaction::Allowed).await? {
            slot.hold(&key);
            return Ok(key);
        }
        before_creating();
        let key = DataKey::generate()?;
        let created = self.creator.create_if_absent(&key).await?;
        let stored = match slot.store.read(Interaction::Allowed).await? {
            Lookup::Found(stored) => stored,
            Lookup::Absent => {
                return Err(StoreError::failed(anyhow!(
                    "The new config key is not in the store after storing it"
                )));
            }
        };
        if created == Created::Stored && !stored.same_as(&key) {
            return Err(StoreError::failed(anyhow!(
                "The store holds another config key than the one just stored"
            )));
        }
        slot.hold(&stored);
        Ok(stored)
    }
}

impl Slot {
    fn held(&self) -> MutexGuard<'_, Held> {
        self.held.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Starts one read of the store, run as a task of its own so it goes on, and fills the
    /// slot, when every reader stopped waiting. However the task ends (an answer, a panic, the
    /// runtime shutting down), its attempt is settled.
    fn ask(self: &Arc<Self>) -> (u64, Shared<BoxFuture<'static, Result<Lookup, StoreError>>>) {
        static ATTEMPTS: AtomicU64 = AtomicU64::new(0);
        let attempt = ATTEMPTS.fetch_add(1, Ordering::Relaxed);
        let mut settle = Settle {
            slot: self.clone(),
            attempt,
            answer: None,
        };
        let task = tokio::spawn(async move {
            let answer = settle.slot.store.read(Interaction::Allowed).await;
            settle.record(&answer);
            answer
        });
        let answer = async move {
            task.await.unwrap_or_else(|err| {
                Err(StoreError::failed(anyhow!(
                    "Reading the key stopped: {err}"
                )))
            })
        }
        .boxed()
        .shared();
        (attempt, answer)
    }

    /// Ends `attempt`: a key found is held, anything else (no answer at all included) leaves
    /// nothing held, so the next reader asks again. A key a writer stored meanwhile, or a newer
    /// attempt, stays.
    fn settle(&self, attempt: u64, answer: Option<&Result<Lookup, StoreError>>) {
        let mut held = self.held();
        if !matches!(&*held, Held::Asking { attempt: current, .. } if *current == attempt) {
            return;
        }
        *held = match answer {
            Some(Ok(Lookup::Found(key))) => Held::Key(key.clone()),
            Some(Ok(Lookup::Absent) | Err(_)) | None => Held::Nothing,
        };
    }

    fn hold(&self, key: &DataKey) {
        *self.held() = Held::Key(key.clone());
    }
}

/// Settles a read attempt when its task ends, whichever way.
struct Settle {
    slot: Arc<Slot>,
    attempt: u64,
    answer: Option<Result<Lookup, StoreError>>,
}

impl Settle {
    fn record(&mut self, answer: &Result<Lookup, StoreError>) {
        self.answer = Some(answer.clone());
    }
}

impl Drop for Settle {
    fn drop(&mut self) {
        self.slot.settle(self.attempt, self.answer.as_ref());
    }
}

/// The OS key's creation lock: in the OS config folder as the system knows it, so every
/// config and every process of the user shares it, else beside the config file.
#[cfg_attr(test, allow(dead_code))]
fn os_key_lock(config_path: Option<&Path>) -> Option<PathBuf> {
    crate::platform::dirs::shared_config_home()
        .map(|dir| dir.join(crate::profile::Profile::Installed.folder()))
        .or_else(|| config_path.and_then(Path::parent).map(Path::to_path_buf))
        .map(|dir| dir.join(OS_KEY_LOCK))
}

/// Takes the lock at `path`, its folder created private, waiting up to `wait` (as long as a
/// settings writer does); a lock another process keeps holding is `Refused`: try again.
fn take_creation_lock(path: &Path, wait: Duration) -> Result<File, StoreError> {
    let locking = |err: std::io::Error| {
        StoreError::failed(anyhow!(err).context(format!("Locking {}", path.display())))
    };
    let (Some(folder), Some(name)) = (path.parent(), path.file_name().and_then(|n| n.to_str()))
    else {
        return Err(StoreError::failed(anyhow!(
            "{} is not a lock file's path",
            path.display()
        )));
    };
    crate::platform::private_file::create_private_folder(folder).map_err(locking)?;
    let file = crate::platform::private_file::open_folder(folder)
        .and_then(|folder| folder.open_lock(name))
        .map_err(locking)?;
    let deadline = Instant::now() + wait;
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(TryLockError::WouldBlock) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(TryLockError::WouldBlock) => {
                return Err(StoreError::refused(anyhow!(
                    "Another process is creating the config key"
                )));
            }
            Err(TryLockError::Error(err)) => return Err(locking(err)),
        }
    }
}
