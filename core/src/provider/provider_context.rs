use anyhow::Result;

use crate::provider::provider_config::{ConfigSource, ProviderConfig, ProviderConfigKey};

#[derive(Debug, Clone, Default)]
pub struct ProviderContext {
    config: ProviderConfig,
}

impl ProviderContext {
    pub fn resolve(source: &dyn ConfigSource, requirements: &[ProviderConfigKey]) -> Self {
        Self {
            config: ProviderConfig::resolve(source, requirements),
        }
    }

    pub fn get(&self, key: ProviderConfigKey) -> Option<&str> {
        self.config.get(key)
    }

    pub fn require(&self, key: ProviderConfigKey) -> Result<&str> {
        self.config.require(key)
    }

    pub fn bool(&self, key: ProviderConfigKey) -> bool {
        self.config.bool(key)
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
