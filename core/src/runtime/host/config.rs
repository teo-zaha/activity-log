//! The config file: edits, reloads and the watcher.

use std::{
    future::Future,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use anyhow::{Context, anyhow};
use tokio::sync::oneshot;

use super::{Host, install::InstallOutcome};
use crate::{
    diagnostics,
    model::{InstallReceipt, ProvidersResponse},
    provider::connection::Connection,
    secrets::Sealer,
    settings::{
        ConfigEditError, ConfigFile, ContentId, EditError, Precondition, edit_config,
        read_config_bytes,
    },
};

/// What a settings edit installed.
pub struct Edit<T> {
    pub value: T,
    /// The file as written, instances included: what a writer's next base is taken from.
    pub file: ConfigFile,
    /// The providers as installed: a connection whose check runs again is checking.
    pub providers: ProvidersResponse,
    /// The providers part's revision that first holds this edit, or the current one when the
    /// edit changed nothing. `None` when the file was written but the host shut down before it
    /// installed it (B70): the save is durable, yet no revision of this host shows it.
    pub installed: Option<InstallReceipt>,
}

/// Why a read of the config file failed, as far as logging it again goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ReadFailure {
    Io(std::io::ErrorKind),
    /// The read's task stopped before it answered.
    Stopped,
}

/// How often the config file's content is compared with the installed one.
pub(super) const RELOAD_INTERVAL: Duration = Duration::from_secs(2);
/// How long a change another writer made must stay the same before it is installed.
pub(super) const SETTLE: Duration = Duration::from_millis(200);

impl Host {
    /// The settings file this host installs, `None` without a settings location.
    pub fn config_path(&self) -> Option<&Path> {
        self.inner.path.as_deref()
    }

    /// Edits the config file (see [`edit_config`]) and installs the result. The write and the
    /// install run in a task of their own, so dropping this future never leaves a written file
    /// uninstalled.
    /// `sealing` is the key an edit that brings a new secret obtained, from this host's
    /// `Secrets`, before anything is locked (`Secrets::sealer`).
    pub async fn edit<T: Send + 'static>(
        &self,
        precondition: Precondition,
        sealing: Option<Sealer>,
        change: impl FnOnce(&mut ConfigFile) -> Result<T, ConfigEditError> + Send + 'static,
    ) -> Result<Edit<T>, EditError> {
        let Some(path) = self.inner.path.clone() else {
            return Err(EditError::NoLocation);
        };
        let host = self.clone();
        let (answer, answered) = oneshot::channel();
        tokio::spawn(async move {
            let _ = answer.send(host.edit_owned(path, precondition, sealing, change).await);
        });
        answered
            .await
            .unwrap_or_else(|_| Err(EditError::Internal(anyhow!("Saving the settings stopped"))))
    }

    async fn edit_owned<T: Send + 'static>(
        self,
        path: PathBuf,
        precondition: Precondition,
        sealing: Option<Sealer>,
        change: impl FnOnce(&mut ConfigFile) -> Result<T, ConfigEditError> + Send + 'static,
    ) -> Result<Edit<T>, EditError> {
        let config = self.inner.config.clone().lock_owned().await;
        if self.lock().shut_down {
            return Err(EditError::Internal(anyhow!(
                "Activity Log is quitting; the settings were not saved"
            )));
        }
        // The config lock may wait for another process, so it is not taken on an async worker.
        let edited = tokio::task::spawn_blocking(move || {
            edit_config(&path, precondition, sealing.as_ref(), change)
        })
        .await
        .map_err(|err| EditError::Internal(anyhow!("Saving the settings failed: {err}")))??;
        #[cfg(test)]
        self.pause(|pauses| pauses.after_edit_write.clone()).await;
        for warning in &edited.warnings {
            diagnostics::warn("config", warning);
        }
        let connections = edited.file.connections().connections;
        let unchanged = self.lock().content == Some(edited.content);
        let applied = if unchanged {
            // Nothing changed anywhere: the providers as they are, which hold the file.
            self.apply(|_, state| InstallOutcome::Shown(state.providers_response()))
        } else {
            self.apply_install(connections, Some(edited.content))
        };
        drop(config);
        let receipt = applied.published.receipt();
        let (providers, installed) = match self.spawn(applied) {
            InstallOutcome::Shown(providers) => (providers, Some(receipt)),
            InstallOutcome::ShutDown(providers) => (providers, None),
        };
        Ok(Edit {
            value: edited.value,
            file: edited.file,
            providers,
            installed,
        })
    }

    /// Installs the config file if its content is not the installed one. A change another
    /// writer made is installed once it read the same twice, [`SETTLE`] apart; content that does
    /// not parse is logged once and the installed config stays.
    pub async fn reload_if_changed(&self) {
        let Some(path) = self.inner.path.clone() else {
            return;
        };
        let config = self.inner.config.clone().lock_owned().await;
        if self.lock().shut_down {
            return;
        }
        let Some(bytes) = self.read_config(&path).await else {
            return;
        };
        let content = ContentId::of(&bytes);
        {
            let state = self.lock();
            if state.content == Some(content) || state.rejected == Some(content) {
                return;
            }
        }
        #[cfg(test)]
        self.pause(|pauses| pauses.after_first_read.clone()).await;
        tokio::time::sleep(SETTLE).await;
        if self.read_config(&path).await.as_ref() != Some(&bytes) {
            // Still being written: the next look installs it.
            return;
        }
        let parsed = ConfigFile::parse(&path, &bytes).map(|file| file.logged_connections());
        let connections = match parsed {
            Ok(connections) => connections,
            Err(err) => {
                diagnostics::error(
                    "config",
                    format!("Could not reload {}: {err:#}", path.display()),
                );
                self.lock().rejected = Some(content);
                return;
            }
        };
        diagnostics::info("config", format!("Reloaded {}", path.display()));
        let applied = self.apply_install(connections, Some(content));
        drop(config);
        self.spawn(applied);
    }

    /// Calls [`Self::reload_if_changed`] every 2 s until shutdown or until the host is gone;
    /// holds the host weakly. Ends at once without a config file.
    pub fn watch_config(&self) -> impl Future<Output = ()> + Send + 'static {
        let host = Arc::downgrade(&self.inner);
        let watched = self.inner.path.is_some();
        let mut stop = self.inner.shutdown.subscribe();
        async move {
            if !watched || *stop.borrow_and_update() {
                return;
            }
            loop {
                tokio::select! {
                    () = tokio::time::sleep(RELOAD_INTERVAL) => {}
                    _ = stop.changed() => return,
                }
                let Some(inner) = host.upgrade() else {
                    return;
                };
                Host { inner }.reload_if_changed().await;
            }
        }
    }

    /// Reads the config file's bytes, a missing file as empty ones. A failure is logged once
    /// per distinct message and tried again on the next look.
    async fn read_config(&self, path: &Path) -> Option<Vec<u8>> {
        let owned = path.to_path_buf();
        let read = tokio::task::spawn_blocking(move || read_config_bytes(&owned)).await;
        let (failure, message) = match read {
            Ok(Ok(bytes)) => {
                self.lock().reload_error = None;
                return Some(bytes.unwrap_or_default());
            }
            Ok(Err(err)) => (ReadFailure::Io(err.kind()), err.to_string()),
            Err(err) => (ReadFailure::Stopped, err.to_string()),
        };
        let first = self.lock().reload_error.replace(failure) != Some(failure);
        if first {
            diagnostics::error(
                "config",
                format!("Could not read {}: {message}", path.display()),
            );
        }
        None
    }
}

/// Reads the connections of the config file at `path`, with the content they were read from
/// when there was any.
pub(super) fn load(
    path: &Path,
) -> Result<(Vec<Connection>, ContentId), (anyhow::Error, Option<ContentId>)> {
    let bytes = read_config_bytes(path)
        .with_context(|| format!("Reading {}", path.display()))
        .map_err(|err| (err, None))?
        .unwrap_or_default();
    let content = ContentId::of(&bytes);
    let connections = ConfigFile::parse(path, &bytes)
        .map(|file| file.logged_connections())
        .map_err(|err| (err.into(), Some(content)))?;
    Ok((connections, content))
}
