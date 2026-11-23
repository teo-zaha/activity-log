//! A generic password, service `Activity Log`, account `config key`: created in the user's
//! default keychain (the login keychain unless the user changed it) and read through the
//! keychain search list, so a changed default does not make it look absent. The program that
//! creates it is on its access list; any other program the user must allow in a dialog naming
//! it.
//!
//! Every keychain call runs under one process-wide lock: [`Interaction::Never`] turns dialogs
//! off for the whole process, so no other call may run beside it.

use std::sync::{Mutex, PoisonError};

use anyhow::anyhow;
use security_framework::os::macos::{keychain::SecKeychain, passwords::find_generic_password};

use super::{ItemName, Place, Status, key_of, macos_status};
use crate::secrets::{Created, DataKey, Interaction, Lookup, StoreError, StoreErrorKind};

const SERVICE: &str = "Activity Log";

fn account(name: ItemName) -> &'static str {
    match name {
        ItemName::ConfigKey => "config key",
        ItemName::Test => "config key (test)",
    }
}

pub(super) async fn read(place: Place, interaction: Interaction) -> Result<Lookup, StoreError> {
    blocking(move || {
        let _no_dialogs = no_dialogs(interaction)?;
        // Opened first, so no default keychain is `Unavailable`, never an empty search; then
        // the tests' own keychain alone, or the search list.
        let opened = keychain(&place)?;
        let keychain = place.keychain.as_ref().map(|_| opened);
        match find_generic_password(
            keychain.as_ref().map(std::slice::from_ref),
            SERVICE,
            account(place.name),
        ) {
            Ok((password, _item)) => key_of(&password),
            Err(err) => match macos_status(err.code()) {
                Status::Absent => Ok(Lookup::Absent),
                _ => Err(error(err, "Reading the config key from the keychain")),
            },
        }
    })
    .await
}

pub(super) async fn create_if_absent(
    place: Place,
    key: DataKey,
    interaction: Interaction,
) -> Result<Created, StoreError> {
    blocking(move || {
        let _no_dialogs = no_dialogs(interaction)?;
        let keychain = keychain(&place)?;
        // Adding refuses an item that exists (`errSecDuplicateItem`): it never overwrites.
        match keychain.add_generic_password(SERVICE, account(place.name), key.bytes()) {
            Ok(()) => Ok(Created::Stored),
            Err(err) => match macos_status(err.code()) {
                Status::Duplicate => Ok(Created::AlreadyThere),
                _ => Err(error(err, "Adding the config key to the keychain")),
            },
        }
    })
    .await
}

pub(super) async fn probe(place: Place) -> Result<(), StoreError> {
    blocking(move || keychain(&place).map(drop)).await
}

#[cfg(feature = "real-store-tests")]
pub(super) async fn delete(place: Place) -> Result<(), StoreError> {
    blocking(move || {
        let keychain = keychain(&place)?;
        match keychain.find_generic_password(SERVICE, account(place.name)) {
            Ok((_password, item)) => {
                item.delete();
                Ok(())
            }
            Err(err) => match macos_status(err.code()) {
                Status::Absent => Ok(()),
                _ => Err(error(err, "Finding the config key to delete it")),
            },
        }
    })
    .await
}

/// Dialogs off for the whole process under `Never`, until the guard is dropped; the lock
/// `blocking` holds keeps every other keychain call of this process out meanwhile.
fn no_dialogs(interaction: Interaction) -> Result<Option<impl Drop>, StoreError> {
    match interaction {
        Interaction::Allowed => Ok(None),
        Interaction::Never => SecKeychain::disable_user_interaction()
            .map(Some)
            .map_err(|err| error(err, "Turning keychain dialogs off")),
    }
}

/// The default keychain, or the tests' own file. Opening a file does not check it exists, so
/// a missing one is told apart here.
fn keychain(place: &Place) -> Result<SecKeychain, StoreError> {
    match &place.keychain {
        None => SecKeychain::default().map_err(|err| error(err, "Opening the default keychain")),
        Some(path) if !path.is_file() => Err(StoreError::unavailable(anyhow!(
            "No keychain at {}",
            path.display()
        ))),
        Some(path) => SecKeychain::open(path).map_err(|err| error(err, "Opening the keychain")),
    }
}

fn error(err: security_framework::base::Error, doing: &str) -> StoreError {
    let kind = match macos_status(err.code()) {
        Status::Error(kind) => kind,
        Status::Absent | Status::Duplicate => StoreErrorKind::Failed,
    };
    StoreError::new(kind, anyhow!("{doing}: {err} (OSStatus {})", err.code()))
}

/// One keychain call at a time in this process (see the module's comment).
static KEYCHAIN: Mutex<()> = Mutex::new(());

/// Keychain calls block while a dialog is open, so they run off the async threads, one at a
/// time. A caller that stops waiting leaves the call to finish; it holds nothing but the answer.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, StoreError> + Send + 'static,
) -> Result<T, StoreError> {
    tokio::task::spawn_blocking(move || {
        let _one_at_a_time = KEYCHAIN.lock().unwrap_or_else(PoisonError::into_inner);
        work()
    })
    .await
    .unwrap_or_else(|err| {
        Err(StoreError::failed(anyhow!(
            "The keychain call stopped: {err}"
        )))
    })
}
