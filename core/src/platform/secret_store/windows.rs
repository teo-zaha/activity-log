//! A generic credential in Credential Manager, `activity-log/config-key`, persisted for this
//! computer only (`CRED_PERSIST_LOCAL_MACHINE`): a roaming profile does not carry the key to a
//! server beside the roaming `config.toml`. Credential Manager never shows a dialog, so both
//! [`Interaction`]s read alike.

use std::ptr;

use anyhow::anyhow;
use windows::{
    Win32::Security::Credentials::{
        CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredFree, CredReadW, CredWriteW,
    },
    core::{PCWSTR, PWSTR},
};

use super::{ItemName, Place, Status, key_of, windows_status};
use crate::secrets::{Created, DataKey, Interaction, Lookup, StoreError, StoreErrorKind};

fn target(name: ItemName) -> Vec<u16> {
    let name = match name {
        ItemName::ConfigKey => "activity-log/config-key",
        ItemName::Test => "activity-log/config-key-test",
    };
    name.encode_utf16().chain(std::iter::once(0)).collect()
}

pub(super) async fn read(place: Place, _interaction: Interaction) -> Result<Lookup, StoreError> {
    blocking(move || read_now(place.name)).await
}

fn read_now(name: ItemName) -> Result<Lookup, StoreError> {
    let target = target(name);
    let mut credential: *mut CREDENTIALW = ptr::null_mut();
    // SAFETY: `target` is NUL-terminated and outlives the call; on success `credential` points
    // at memory the system allocated, freed below.
    let read = unsafe {
        CredReadW(
            PCWSTR(target.as_ptr()),
            CRED_TYPE_GENERIC,
            None,
            &mut credential,
        )
    };
    if let Err(err) = read {
        return match windows_status(win32_code(&err)) {
            Status::Absent => Ok(Lookup::Absent),
            _ => Err(error(
                &err,
                "Reading the config key from Credential Manager",
            )),
        };
    }
    // SAFETY: the system returned a valid credential whose blob has `CredentialBlobSize` bytes.
    unsafe {
        let blob = (*credential).CredentialBlob;
        let size = (*credential).CredentialBlobSize as usize;
        let bytes = if blob.is_null() {
            &[][..]
        } else {
            std::slice::from_raw_parts(blob, size)
        };
        let found = key_of(bytes);
        CredFree(credential.cast());
        found
    }
}

pub(super) async fn create_if_absent(place: Place, key: DataKey) -> Result<Created, StoreError> {
    blocking(move || {
        // `CredWriteW` replaces a credential that exists, so it is looked for first; the
        // key-creation lock keeps our other processes out meanwhile.
        if let Lookup::Found(_) = read_now(place.name)? {
            return Ok(Created::AlreadyThere);
        }
        let mut target = target(place.name);
        let mut blob = *key.bytes();
        let credential = CREDENTIALW {
            Type: CRED_TYPE_GENERIC,
            TargetName: PWSTR(target.as_mut_ptr()),
            CredentialBlobSize: blob.len() as u32,
            CredentialBlob: blob.as_mut_ptr(),
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            ..Default::default()
        };
        // SAFETY: every pointer in `credential` points into `target` and `blob`, alive here.
        let written = unsafe { CredWriteW(&credential, 0) };
        zeroize::Zeroize::zeroize(&mut blob);
        written
            .map(|()| Created::Stored)
            .map_err(|err| error(&err, "Writing the config key to Credential Manager"))
    })
    .await
}

/// Reading is the probe: it never shows a dialog, and a missing item says the store answers.
pub(super) async fn probe(place: Place) -> Result<(), StoreError> {
    blocking(move || read_now(place.name).map(drop)).await
}

#[cfg(feature = "real-store-tests")]
pub(super) async fn delete(place: Place) -> Result<(), StoreError> {
    use windows::Win32::Security::Credentials::CredDeleteW;
    blocking(move || {
        let target = target(place.name);
        // SAFETY: `target` is NUL-terminated and outlives the call.
        match unsafe { CredDeleteW(PCWSTR(target.as_ptr()), CRED_TYPE_GENERIC, None) } {
            Ok(()) => Ok(()),
            Err(err) => match windows_status(win32_code(&err)) {
                Status::Absent => Ok(()),
                _ => Err(error(
                    &err,
                    "Deleting the config key from Credential Manager",
                )),
            },
        }
    })
    .await
}

/// The Win32 error code inside a `HRESULT_FROM_WIN32` value; another facility's code is kept
/// whole, which no rule matches.
fn win32_code(err: &windows::core::Error) -> u32 {
    const FACILITY_WIN32: u32 = 7;
    let code = err.code().0 as u32;
    if (code >> 16) & 0x1fff == FACILITY_WIN32 {
        code & 0xffff
    } else {
        code
    }
}

fn error(err: &windows::core::Error, doing: &str) -> StoreError {
    let kind = match windows_status(win32_code(err)) {
        Status::Error(kind) => kind,
        Status::Absent | Status::Duplicate => StoreErrorKind::Failed,
    };
    StoreError::new(kind, anyhow!("{doing}: {err}"))
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, StoreError> + Send + 'static,
) -> Result<T, StoreError> {
    tokio::task::spawn_blocking(work)
        .await
        .unwrap_or_else(|err| {
            Err(StoreError::failed(anyhow!(
                "The Credential Manager call stopped: {err}"
            )))
        })
}
