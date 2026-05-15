use std::path::PathBuf;

use anyhow::Result;

use crate::provider::provider_config::{
    ConfigBinding, ProviderConfig, ProviderConfigKey, ProviderConfigResolve,
};

pub const GIT_DIRS: ProviderConfigKey = ProviderConfigKey::optional_string("GIT_DIRS");
const CONFIG_KEYS: &[ProviderConfigKey] = &[GIT_DIRS];

#[derive(Debug, Clone)]
pub struct GitConfig {
    pub dirs: Option<Vec<PathBuf>>,
}

impl GitConfig {
    pub fn binding() -> ConfigBinding {
        ConfigBinding::of::<Self>()
    }
}

impl ProviderConfigResolve for GitConfig {
    fn requirements() -> &'static [ProviderConfigKey] {
        CONFIG_KEYS
    }

    fn from_config(config: &ProviderConfig) -> Result<Self> {
        let dirs = config.get(GIT_DIRS).and_then(|value| {
            let paths: Vec<PathBuf> = value
                .split(',')
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
                .collect();
            (!paths.is_empty()).then_some(paths)
        });
        Ok(Self { dirs })
    }
}
