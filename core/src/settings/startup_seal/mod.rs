//! Sealing the plain-text secrets a file holds (hand-written, or from a build before sealing),
//! at the desktop app's start, before its host first reads the file: only where the key comes
//! without a dialog (`Interaction::Never`), and never at the cost of starting.

use std::path::Path;

use super::{ConfigFile, Precondition, edit_config};
use crate::{
    diagnostics,
    secrets::{Interaction, Secrets},
};

#[cfg(test)]
mod tests;

/// What the pass did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupSeal {
    /// The file holds no plain secret, or there is no file.
    NothingToSeal,
    Sealed,
    /// The key needs a dialog, or the store gave none, or the edit failed: the plain text
    /// stays, and the next token saved seals it.
    Skipped,
}

/// Seals every plain secret in the file at `path` with the key `secrets` gives without a
/// dialog. Everything it meets is logged; it never fails.
pub async fn seal_without_asking(path: &Path, secrets: &Secrets) -> StartupSeal {
    let file = match ConfigFile::load(path) {
        Ok(file) => file,
        Err(_) => return StartupSeal::NothingToSeal,
    };
    if file.plain_secrets().is_empty() {
        return StartupSeal::NothingToSeal;
    }
    let sealer = match secrets
        .sealer(Interaction::Never, |store| file.sealed_in(store))
        .await
    {
        Ok(sealer) => sealer,
        Err(err) => {
            diagnostics::info(
                "secrets",
                format!("Plain-text secrets stay so until a token is saved: {err}"),
            );
            return StartupSeal::Skipped;
        }
    };
    let path = path.to_path_buf();
    let edited = tokio::task::spawn_blocking(move || {
        edit_config(&path, Precondition::None, Some(&sealer), |_| Ok(()))
    })
    .await;
    match edited {
        Ok(Ok(_)) => {
            diagnostics::info("secrets", "Sealed the plain-text secrets in config.toml");
            StartupSeal::Sealed
        }
        Ok(Err(err)) => {
            diagnostics::warn(
                "secrets",
                format!("Sealing the plain-text secrets failed: {err}"),
            );
            StartupSeal::Skipped
        }
        Err(err) => {
            diagnostics::warn(
                "secrets",
                format!("Sealing the plain-text secrets stopped: {err}"),
            );
            StartupSeal::Skipped
        }
    }
}
