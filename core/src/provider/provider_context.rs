use std::any::Any;
use std::fmt;
use std::sync::Arc;

use anyhow::{Result, anyhow};

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
            typed: None,
            connection: None,
            collection_state: None,
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

    pub fn first_invalid_key<'a>(
        &self,
        requirements: &'a [ProviderConfigKey],
    ) -> Option<&'a ProviderConfigKey> {
        self.config.first_invalid_key(requirements)
    }
}
