use std::any::Any;
use std::fmt;
use std::sync::Arc;

use anyhow::{Result, anyhow};

use crate::provider::provider_config::{
    ConfigSource, ProviderConfig, ProviderConfigKey, ProviderConfigResolve,
};

#[derive(Clone, Default)]
pub struct ProviderContext {
    config: ProviderConfig,
    typed: Option<Arc<dyn Any + Send + Sync>>,
    cache_scope: u64,
}

impl fmt::Debug for ProviderContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProviderContext")
            .field("config", &self.config)
            .finish()
    }
}

impl ProviderContext {
    pub fn resolve(source: &dyn ConfigSource, requirements: &[ProviderConfigKey]) -> Self {
        let config = ProviderConfig::resolve(source, requirements);
        let cache_scope = config.fingerprint(requirements);
        Self {
            config,
            typed: None,
            cache_scope,
        }
    }

    pub fn with_typed<T: ProviderConfigResolve>(&mut self) -> Result<()> {
        let parsed = T::from_config(&self.config)?;
        self.typed = Some(Arc::new(parsed));
        Ok(())
    }

    pub fn config<T: 'static>(&self) -> Result<&T> {
        self.typed
            .as_ref()
            .and_then(|any| any.downcast_ref::<T>())
            .ok_or_else(|| anyhow!("Config not resolved or wrong type"))
    }

    pub(crate) fn cache_scope(&self) -> u64 {
        self.cache_scope
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
