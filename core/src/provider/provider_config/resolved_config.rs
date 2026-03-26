use std::collections::BTreeMap;

use anyhow::{Result, anyhow};
use reqwest::Url;

use super::{ConfigSource, ProviderConfigKey, ProviderConfigValueType};

#[derive(Debug, Clone, Default)]
pub struct ProviderConfig {
    values: BTreeMap<String, String>,
}

impl ProviderConfig {
    pub fn resolve(source: &dyn ConfigSource, requirements: &[ProviderConfigKey]) -> Self {
        let values = requirements
            .iter()
            .filter_map(|key| {
                source
                    .get(key.name)
                    .map(|value| (key.name.to_string(), value))
            })
            .collect();
        Self { values }
    }

    pub fn get(&self, key: ProviderConfigKey) -> Option<&str> {
        self.values.get(key.name()).map(String::as_str)
    }

    pub fn require(&self, key: ProviderConfigKey) -> Result<&str> {
        self.get(key)
            .ok_or_else(|| anyhow!("Missing provider config: {}", key.name()))
    }

    pub fn bool(&self, key: ProviderConfigKey) -> bool {
        self.get(key).and_then(parse_bool).unwrap_or(false)
    }

    pub fn missing_required_keys<'a>(&self, requirements: &'a [ProviderConfigKey]) -> Vec<&'a str> {
        requirements
            .iter()
            .filter(|key| key.required && !self.values.contains_key(key.name))
            .map(|key| key.name)
            .collect()
    }

    pub fn first_invalid_key<'a>(
        &self,
        requirements: &'a [ProviderConfigKey],
    ) -> Option<&'a ProviderConfigKey> {
        requirements.iter().find(|key| {
            self.values
                .get(key.name)
                .is_some_and(|value| !value_matches_type(value, key.value_type))
        })
    }
}

fn value_matches_type(value: &str, value_type: ProviderConfigValueType) -> bool {
    match value_type {
        ProviderConfigValueType::String | ProviderConfigValueType::Password => true,
        ProviderConfigValueType::Bool => parse_bool(value).is_some(),
        ProviderConfigValueType::Int => value.parse::<i64>().is_ok(),
        ProviderConfigValueType::Email => looks_like_email(value),
        ProviderConfigValueType::Url => Url::parse(value).is_ok(),
    }
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Some(true),
        "false" | "0" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn looks_like_email(value: &str) -> bool {
    let (local, domain) = match value.split_once('@') {
        Some(parts) => parts,
        None => return false,
    };

    !local.is_empty()
        && !domain.is_empty()
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && domain.contains('.')
}
