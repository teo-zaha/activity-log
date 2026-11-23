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

use crate::model::SecretStoreInfo;
use anyhow::anyhow;
use futures::{
    FutureExt,
    future::{BoxFuture, Shared},
};

mod choice;
mod key_file;
#[cfg(any(test, feature = "test-support"))]
mod memory;
pub mod opening;
mod os_store;
pub mod sealed;
mod store;
#[cfg(test)]
mod tests;

use choice::Decided;
pub use choice::{Choice, ForcedStore, FrontEnd, OsAnswer, SECRET_STORE_VARIABLE, UnknownStore};
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

/// What a check waits for while the OS store may show a dialog.
#[cfg(target_os = "macos")]
const OS_STORE_WAIT: &str = "Waiting for the keychain";
#[cfg(target_os = "windows")]
const OS_STORE_WAIT: &str = "Waiting for Credential Manager";
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
const OS_STORE_WAIT: &str = "Waiting for the keyring";

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The stores of one process and the keys read from them. Cheap to clone; clones share both.
#[derive(Clone)]
pub struct Secrets(Arc<Inner>);

struct Inner {
    /// `None` without an OS store: the dev profile, a test.
    os: Option<Arc<Slot>>,
    file: Arc<Slot>,
    choice: Choice,
    /// How the OS store last answered (a probe or a read), for what the settings show.
    os_answer: Mutex<OsAnswer>,
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
    interaction: Interaction,
}

/// The key a writer seals new secrets with, and the store that holds it; read fresh for the
/// one edit it is for.
pub struct Sealer {
    store: StoreKind,
    key: DataKey,
}

impl Sealer {
    /// A sealer over `key`, for tests of the writers.
    #[cfg(any(test, feature = "test-support"))]
    pub fn for_tests(store: StoreKind, key: DataKey) -> Self {
        Self { store, key }
    }

    pub fn store(&self) -> StoreKind {
        self.store
    }

    pub fn seal(
        &self,
        field: crate::provider::provider_config::ProviderConfigKey,
        binding: &sealed::Binding,
        secret: &str,
    ) -> Result<sealed::SealedValue, sealed::SealError> {
        sealed::seal(&self.key, self.store, field, binding, secret)
    }

    /// Whether `value` was sealed with this key.
    pub fn sealed(&self, value: &sealed::SealedValue) -> bool {
        value.store() == self.store && value.key_id() == sealed::KeyId::of(&self.key)
    }

    pub fn open(
        &self,
        value: &sealed::SealedValue,
        field: crate::provider::provider_config::ProviderConfigKey,
        binding: &sealed::Binding,
    ) -> Result<zeroize::Zeroizing<String>, sealed::Unopenable> {
        sealed::open(value, &self.key, field, binding)
    }
}

impl std::fmt::Debug for Sealer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sealer")
            .field("store", &self.store)
            .finish_non_exhaustive()
    }
}

/// The desktop app without an OS store, or a CLI held to it: no store to seal with.
fn no_store() -> StoreError {
    StoreError::unavailable(anyhow!(
        "There is no keychain or other secure store on this computer; install or start a keyring service (GNOME Keyring, KWallet, KeePassXC)"
    ))
}

/// The CLI's fallback to the key file, said once per process.
fn warn_falling_back(err: &StoreError) {
    static SAID: std::sync::Once = std::sync::Once::new();
    SAID.call_once(|| {
        crate::diagnostics::warn(
            "secrets",
            format!("No keychain ({err}): secrets are protected by a key file instead"),
        );
    });
}

/// A store for a handle: the store and its key-creation lock.
pub type StoreSlot = (Arc<dyn SecretStore>, Option<PathBuf>);

impl Secrets {
    /// The desktop app's: the OS store, and the key file read only. With the dev profile, as
    /// for every front end, only the profile's own key file, which it creates.
    #[cfg(not(test))]
    pub fn desktop(config_path: Option<&Path>) -> Self {
        Self::of_user(FrontEnd::Desktop, config_path, KeyFileAccess::ReadOnly)
    }

    /// The CLI's: the OS store, and a key file it may create.
    #[cfg(not(test))]
    pub fn cli(config_path: Option<&Path>) -> Self {
        Self::of_user(FrontEnd::Cli, config_path, KeyFileAccess::Create)
    }

    /// The API server's: it never writes the config, so it never creates a key.
    #[cfg(not(test))]
    pub fn api(config_path: Option<&Path>) -> Self {
        Self::of_user(FrontEnd::Api, config_path, KeyFileAccess::ReadOnly)
    }

    /// The stores the user's programs use. In a build with `test-support` (another crate's
    /// tests, and the binaries those tests run) there is no OS store at all and the key file is
    /// only where `ACTIVITY_LOG_SECRET_KEY_FILE` puts it, so no test reaches a real keychain or
    /// key file.
    #[cfg(not(test))]
    fn of_user(front_end: FrontEnd, config_path: Option<&Path>, access: KeyFileAccess) -> Self {
        let profile = crate::profile::current();
        let forced = match front_end {
            FrontEnd::Desktop => {
                if std::env::var_os(SECRET_STORE_VARIABLE).is_some() {
                    crate::diagnostics::warn(
                        "secrets",
                        format!(
                            "{SECRET_STORE_VARIABLE} is ignored: the desktop app keeps the key in the OS store"
                        ),
                    );
                }
                None
            }
            FrontEnd::Cli | FrontEnd::Api => {
                let value = std::env::var(SECRET_STORE_VARIABLE).ok();
                ForcedStore::parse(value.as_deref()).unwrap_or_else(|unknown| {
                    crate::diagnostics::warn("secrets", unknown.to_string());
                    None
                })
            }
        };
        let tested = cfg!(feature = "test-support");
        let os_store = matches!(profile, crate::profile::Profile::Installed) && !tested;
        let access = match profile {
            crate::profile::Profile::Dev => KeyFileAccess::Create,
            crate::profile::Profile::Installed => access,
        };
        let os = os_store.then(|| {
            (
                Arc::new(OsStore::user()) as Arc<dyn SecretStore>,
                os_key_lock(config_path),
            )
        });
        let key_file = if tested {
            crate::platform::dirs::env_path(KEY_FILE_VARIABLE)
        } else {
            KeyFileStore::default_path()
        };
        let file = KeyFileStore::new(key_file, access);
        let file_lock = file.creation_lock();
        Self::build(
            Choice {
                front_end,
                profile,
                forced,
                has_os_store: os.is_some(),
            },
            os,
            (Arc::new(file), file_lock),
        )
    }

    /// Any stores, each with its key-creation lock, for `front_end` on the installed profile.
    #[cfg(any(test, feature = "test-support"))]
    pub fn with_stores(front_end: FrontEnd, os: Option<StoreSlot>, file: StoreSlot) -> Self {
        let choice = Choice {
            front_end,
            profile: crate::profile::Profile::Installed,
            forced: None,
            has_os_store: os.is_some(),
        };
        Self::build(choice, os, file)
    }

    /// A handle with no store: nothing sealed opens and nothing is sealed, for a host of
    /// tests.
    #[cfg(any(test, feature = "test-support"))]
    pub fn none() -> Self {
        Self::with_stores(
            FrontEnd::Api,
            None,
            (
                Arc::new(KeyFileStore::new(None, KeyFileAccess::ReadOnly)),
                None,
            ),
        )
    }

    fn build(choice: Choice, os: Option<StoreSlot>, (file, file_lock): StoreSlot) -> Self {
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
            choice,
            os_answer: Mutex::new(OsAnswer::NotAsked),
        }))
    }

    /// Where new secrets are sealed here, as the settings show it, from the OS store's last
    /// answer.
    pub fn store_info(&self) -> SecretStoreInfo {
        self.0.choice.info(*lock(&self.0.os_answer))
    }

    /// Asks the OS store whether it is there and answering, without a dialog, where the
    /// answer decides anything, and returns what the settings show then.
    pub async fn probe(&self) -> SecretStoreInfo {
        if let (Decided::OsFirst, Some(os)) = (self.0.choice.decided(), &self.0.os) {
            let answer = os.store.probe().await;
            self.record_os(answer.as_ref().map(drop));
        }
        self.store_info()
    }

    /// What a connection waits for while its check opens `values`: the OS store, when a value
    /// sealed with its key is there and the key is not held yet. Words for people only.
    pub fn waiting_reason(
        &self,
        values: &crate::provider::connection::ConnectionValues,
    ) -> Option<&'static str> {
        use crate::provider::connection::StoredSecret;
        let needs_os = values.sealed().any(|(_, secret)| {
            matches!(secret, StoredSecret::Sealed(value) if value.store() == StoreKind::Os)
        });
        (needs_os && self.0.os.is_some() && self.held(StoreKind::Os).is_none())
            .then_some(OS_STORE_WAIT)
    }

    fn record_os(&self, answer: Result<(), &StoreError>) {
        *lock(&self.0.os_answer) = match answer {
            Ok(()) => OsAnswer::Ready,
            Err(err) => OsAnswer::Error(err.kind()),
        };
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
        let answer = match tokio::time::timeout(KEY_WAIT, answer).await {
            Ok(Ok(Lookup::Found(key))) => Ok(Some(key)),
            Ok(Ok(Lookup::Absent)) => Ok(None),
            Ok(Err(err)) => Err(err),
            Err(_) => Err(StoreError::failed(anyhow!(
                "The secure store did not answer within {} s",
                KEY_WAIT.as_secs()
            ))),
        };
        if kind == StoreKind::Os {
            self.record_os(answer.as_ref().map(drop));
        }
        answer
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
        self.fresh_key_with(kind, Interaction::Allowed).await
    }

    /// [`Self::fresh_key`], a dialog allowed or not; a creation it leads to asks the same.
    pub async fn fresh_key_with(
        &self,
        kind: StoreKind,
        interaction: Interaction,
    ) -> Result<FreshKey<'_>, StoreError> {
        let slot = self.slot_ref(kind)?;
        let read = slot.store.read(interaction).await;
        if kind == StoreKind::Os {
            self.record_os(read.as_ref().map(drop));
        }
        match read? {
            Lookup::Found(key) => {
                slot.hold(&key);
                Ok(FreshKey::Found(key))
            }
            Lookup::Absent => Ok(match slot.store.creator() {
                Some(creator) => FreshKey::Absent(KeyCreation {
                    slot,
                    creator,
                    interaction,
                }),
                None => FreshKey::AbsentReadOnly,
            }),
        }
    }

    /// The key a writer seals new secrets with, read again from the store this front end
    /// seals with (`secrets/choice`), and created there over a definite absence: then
    /// `orphans(kind)` counts the file's values sealed with that store, which no key here
    /// opens any more, and one warning says how many must be entered again. Where the OS
    /// store is missing the CLI falls back to the key file, saying so once; the desktop app
    /// does not, and nothing falls back over a refusal.
    pub async fn sealer(
        &self,
        interaction: Interaction,
        orphans: impl Fn(StoreKind) -> usize,
    ) -> Result<Sealer, StoreError> {
        let choice = self.0.choice;
        let store = match choice.decided() {
            Decided::File(_) => StoreKind::File,
            Decided::Nothing(_) => return Err(no_store()),
            Decided::OsFirst => match self.sealer_in(StoreKind::Os, interaction, &orphans).await {
                Err(err) if err.kind() == StoreErrorKind::Unavailable => {
                    match choice.os_unavailable() {
                        Decided::File(_) => {
                            warn_falling_back(&err);
                            StoreKind::File
                        }
                        Decided::Nothing(_) | Decided::OsFirst => return Err(err),
                    }
                }
                sealed => return sealed,
            },
        };
        self.sealer_in(store, interaction, &orphans).await
    }

    async fn sealer_in(
        &self,
        store: StoreKind,
        interaction: Interaction,
        orphans: &impl Fn(StoreKind) -> usize,
    ) -> Result<Sealer, StoreError> {
        let key = match self.fresh_key_with(store, interaction).await? {
            FreshKey::Found(key) => key,
            FreshKey::Absent(creation) => {
                creation
                    .create(|| {
                        let count = orphans(store);
                        if count > 0 {
                            crate::diagnostics::warn(
                                "secrets",
                                format!(
                                    "config.toml has {count} secret(s) sealed with a key this computer does not have; a new key is created and those secrets must be entered again here"
                                ),
                            );
                        }
                    })
                    .await?
            }
            FreshKey::AbsentReadOnly => {
                return Err(StoreError::failed(anyhow!(
                    "This program only reads the key file"
                )));
            }
        };
        Ok(Sealer { store, key })
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
        if let Lookup::Found(key) = slot.store.read(self.interaction).await? {
            slot.hold(&key);
            return Ok(key);
        }
        before_creating();
        let key = DataKey::generate()?;
        let created = self.creator.create_if_absent(&key).await?;
        let stored = match slot.store.read(self.interaction).await? {
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
