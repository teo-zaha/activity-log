use std::any::Any;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Result, anyhow};

use crate::model::{IdentityEvidence, RemoteIdentity};
use crate::provider::{
    connection::{Connection, ConnectionLocation},
    provider_adapter::CollectionState,
    provider_config::{ConfigSource, ProviderConfig, ProviderConfigKey, ProviderConfigResolve},
};

/// A kind's typed config, as `provider_config!` declares it; its `Debug` redacts secrets.
trait TypedConfig: Any + Send + Sync + fmt::Debug {}

impl<T: Any + Send + Sync + fmt::Debug> TypedConfig for T {}

#[derive(Clone, Default)]
pub struct ProviderContext {
    config: ProviderConfig,
    typed: Option<Arc<dyn TypedConfig>>,
    connection: Option<Connection>,
    collection_state: Option<CollectionState>,
    identity: IdentityEvidence,
    state_dir: Option<PathBuf>,
}

impl fmt::Debug for ProviderContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProviderContext")
            .field("config", &self.config)
            .field("typed", &self.typed)
            .finish()
    }
}

impl ProviderContext {
    pub fn resolve(source: &dyn ConfigSource, requirements: &[ProviderConfigKey]) -> Self {
        Self {
            config: ProviderConfig::resolve(source, requirements),
            ..Self::default()
        }
    }

    /// The context of one connection: its values and where it lives.
    pub fn for_connection(connection: &Connection, requirements: &[ProviderConfigKey]) -> Self {
        Self {
            connection: Some(connection.clone()),
            ..Self::resolve(connection.values(), requirements)
        }
    }

    /// The context of one connection whose values are read from `source`: its values with
    /// its sealed secrets opened.
    pub fn for_connection_with(
        connection: &Connection,
        source: &dyn ConfigSource,
        requirements: &[ProviderConfigKey],
    ) -> Self {
        Self {
            connection: Some(connection.clone()),
            ..Self::resolve(source, requirements)
        }
    }

    pub fn connection(&self) -> Option<&Connection> {
        self.connection.as_ref()
    }

    pub fn location(&self) -> ConnectionLocation {
        self.connection
            .as_ref()
            .map(|connection| connection.location.clone())
            .unwrap_or_default()
    }

    pub(crate) fn with_collection_state(mut self, state: Option<CollectionState>) -> Self {
        self.collection_state = state;
        self
    }

    pub(crate) fn set_state_dir(&mut self, dir: Option<PathBuf>) {
        self.state_dir = dir;
    }

    /// State beside the day cache, scoped to the connection and identity across config edits.
    pub(crate) fn identity_state_path(
        &self,
        identity: &RemoteIdentity,
        extension: &str,
    ) -> Option<PathBuf> {
        let connection = &self.connection()?.id;
        let dir: &Path = self.state_dir.as_deref()?;
        let scope = crate::utils::fnv1a(identity.id.bytes());
        let mut path = dir.join(crate::cache::file_name(connection, scope));
        path.set_extension(extension);
        Some(path)
    }

    pub(crate) fn set_identity(&mut self, identity: IdentityEvidence) {
        self.identity = identity;
    }

    /// The startup check's evidence for the token's owner, once the check has finished.
    pub fn identity(&self) -> &IdentityEvidence {
        &self.identity
    }

    /// The kind's state for the running collection, see `ProviderAdapter::collection_state`.
    pub fn collection_state<T: 'static>(&self) -> Option<&T> {
        self.collection_state
            .as_ref()
            .and_then(|state| state.downcast_ref::<T>())
    }

    pub fn with_typed<T: ProviderConfigResolve>(&mut self) -> Result<()> {
        let parsed = T::from_config(&self.config)?;
        self.typed = Some(Arc::new(parsed));
        Ok(())
    }

    pub fn config<T: 'static>(&self) -> Result<&T> {
        self.typed
            .as_deref()
            .and_then(|typed| (typed as &dyn Any).downcast_ref::<T>())
            .ok_or_else(|| anyhow!("Config not resolved or wrong type"))
    }

    /// See [`ProviderConfig::fingerprint`].
    pub(crate) fn fingerprint(
        &self,
        requirements: &[ProviderConfigKey],
    ) -> Result<u64, &'static str> {
        self.config.fingerprint(requirements)
    }

    pub fn missing_required_keys<'a>(&self, requirements: &'a [ProviderConfigKey]) -> Vec<&'a str> {
        self.config.missing_required_keys(requirements)
    }

    pub fn first_config_problem(&self, requirements: &[ProviderConfigKey]) -> Option<String> {
        self.config.first_config_problem(requirements)
    }
}
