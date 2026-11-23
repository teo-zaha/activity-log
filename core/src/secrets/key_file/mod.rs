//! The key file: the config key in a file only the user can open, where no OS store can be
//! used (a headless Linux, a CI runner, Docker; the dev profile). It is `secret.key` in a local
//! folder that never roams with a profile, or `ACTIVITY_LOG_SECRET_KEY_FILE`.

use std::{
    io,
    path::{Path, PathBuf},
};

use anyhow::anyhow;
use futures::{FutureExt, future::BoxFuture};
use zeroize::Zeroizing;

use super::{
    Created, DataKey, Interaction, KEY_LEN, KeyCreator, Lookup, SecretStore, StoreError, StoreKind,
};
use crate::platform::{
    dirs,
    private_file::{self, OwnerOnly, ReplaceError},
};

#[cfg(test)]
mod tests;

/// Overrides where the key file is (a Docker volume, a built binary's tests).
pub const KEY_FILE_VARIABLE: &str = "ACTIVITY_LOG_SECRET_KEY_FILE";

const HEADER: &str = "activity-log config key v1\n";

/// Whether this front end may create the key file. The desktop app only reads it: it seals
/// only with the OS store, and opens what the CLI sealed with the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyFileAccess {
    ReadOnly,
    Create,
}

pub struct KeyFileStore {
    /// `None` without a local data folder and without the variable: there is no key file here.
    path: Option<PathBuf>,
    /// `None` where this program only reads the key file.
    writer: Option<KeyFileWriter>,
}

/// The right to create the key file, which only a `KeyFileAccess::Create` store holds.
struct KeyFileWriter {
    path: Option<PathBuf>,
}

impl KeyFileStore {
    pub fn new(path: Option<PathBuf>, access: KeyFileAccess) -> Self {
        let writer = match access {
            KeyFileAccess::Create => Some(KeyFileWriter { path: path.clone() }),
            KeyFileAccess::ReadOnly => None,
        };
        Self { path, writer }
    }

    /// `ACTIVITY_LOG_SECRET_KEY_FILE`, else `activity-log/secret.key` in the local data folder.
    pub fn default_path() -> Option<PathBuf> {
        dirs::env_path(KEY_FILE_VARIABLE).or_else(|| {
            dirs::local_data_home().map(|dir| dir.join("activity-log").join("secret.key"))
        })
    }

    /// The key-creation lock for this key: `{name}.lock` beside the file.
    pub fn creation_lock(&self) -> Option<PathBuf> {
        let path = self.path.as_ref()?;
        let name = path.file_name()?.to_str()?;
        Some(path.with_file_name(format!("{name}.lock")))
    }
}

/// The key file's folder and name.
fn located(path: Option<&Path>) -> Result<(PathBuf, String), StoreError> {
    let path =
        path.ok_or_else(|| StoreError::unavailable(anyhow!("There is no folder for a key file")))?;
    let parts = path
        .parent()
        .zip(path.file_name().and_then(|name| name.to_str()));
    parts
        .map(|(folder, name)| (folder.to_path_buf(), name.to_string()))
        .ok_or_else(|| StoreError::failed(anyhow!("{} is not a key file's path", path.display())))
}

impl SecretStore for KeyFileStore {
    fn kind(&self) -> StoreKind {
        StoreKind::File
    }

    fn read(&self, _interaction: Interaction) -> BoxFuture<'_, Result<Lookup, StoreError>> {
        let located = located(self.path.as_deref());
        async move {
            let (folder, name) = located?;
            blocking(move || read_key(&folder, &name)).await
        }
        .boxed()
    }

    /// The key file is there wherever it has a location.
    fn probe(&self) -> BoxFuture<'_, Result<(), StoreError>> {
        let located = located(self.path.as_deref()).map(drop);
        async move { located }.boxed()
    }

    fn creator(&self) -> Option<&dyn KeyCreator> {
        self.writer.as_ref().map(|writer| writer as &dyn KeyCreator)
    }
}

impl KeyCreator for KeyFileWriter {
    fn create_if_absent<'a>(
        &'a self,
        key: &'a DataKey,
    ) -> BoxFuture<'a, Result<Created, StoreError>> {
        let located = located(self.path.as_deref());
        let key = key.clone();
        async move {
            let (folder, name) = located?;
            blocking(move || create_key(&folder, &name, &key)).await
        }
        .boxed()
    }
}

/// A key file another account could have planted, read or changed is no key to seal with: its
/// folder must be private, and the file itself only the user's, judged on the handle read.
fn read_key(folder_path: &Path, name: &str) -> Result<Lookup, StoreError> {
    let reading = |err: io::Error| {
        StoreError::failed(
            anyhow!(err).context(format!("Reading the key file in {}", folder_path.display())),
        )
    };
    let folder = match private_file::open_folder(folder_path) {
        Ok(folder) => folder,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Lookup::Absent),
        Err(err) => return Err(reading(err)),
    };
    let path = folder_path.join(name);
    let bytes = match folder.read_owner_only(name).map_err(reading)? {
        None => return Ok(Lookup::Absent),
        Some(OwnerOnly::Exposed) => {
            return Err(StoreError::failed(anyhow!(
                "The key file {} is not only this user's: another account may read or change it",
                path.display()
            )));
        }
        Some(OwnerOnly::Private(bytes)) => Zeroizing::new(bytes),
    };
    folder.check_private().map_err(|err| {
        StoreError::failed(anyhow!(err).context("The key file's folder is not private"))
    })?;
    decode(&bytes)
        .map(Lookup::Found)
        .ok_or_else(|| StoreError::failed(anyhow!("{} is not a key file", path.display())))
}

fn create_key(folder_path: &Path, name: &str, key: &DataKey) -> Result<Created, StoreError> {
    let failed = |err: io::Error| {
        StoreError::failed(anyhow!(err).context(format!(
            "Creating the key file in {}",
            folder_path.display()
        )))
    };
    private_file::create_private_folder(folder_path).map_err(failed)?;
    let folder = private_file::open_folder(folder_path).map_err(failed)?;
    // `replace` would overwrite: under the key-creation lock, a file that is there stays.
    if folder
        .read(name)
        .map_err(failed)?
        .map(Zeroizing::new)
        .is_some()
    {
        return Ok(Created::AlreadyThere);
    }
    folder
        .replace(name, encode(key).as_bytes())
        .map_err(|err| match err {
            ReplaceError::Unprotectable { .. } => StoreError::failed(
                anyhow!(err).context("The key file cannot be kept private there"),
            ),
            ReplaceError::Failed { .. } => {
                StoreError::failed(anyhow!(err).context("Writing the key file"))
            }
        })?;
    Ok(Created::Stored)
}

/// The file's text: a header naming the format, then the key in hex.
fn encode(key: &DataKey) -> Zeroizing<String> {
    let mut hex = Zeroizing::new([0u8; KEY_LEN * 2]);
    hex::encode_to_slice(key.bytes(), hex.as_mut_slice()).expect("the buffer fits the key");
    let mut text = Zeroizing::new(String::with_capacity(HEADER.len() + hex.len() + 1));
    text.push_str(HEADER);
    text.push_str(std::str::from_utf8(hex.as_slice()).expect("hex is ASCII"));
    text.push('\n');
    text
}

fn decode(bytes: &[u8]) -> Option<DataKey> {
    let hex = bytes.strip_prefix(HEADER.as_bytes())?;
    let hex = hex.strip_suffix(b"\n").unwrap_or(hex);
    let hex = hex.strip_suffix(b"\r").unwrap_or(hex);
    let mut key = Zeroizing::new([0u8; KEY_LEN]);
    hex::decode_to_slice(hex, key.as_mut_slice()).ok()?;
    DataKey::from_slice(key.as_slice())
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, StoreError> + Send + 'static,
) -> Result<T, StoreError> {
    tokio::task::spawn_blocking(work)
        .await
        .unwrap_or_else(|err| {
            Err(StoreError::failed(anyhow!(
                "The key file work stopped: {err}"
            )))
        })
}
