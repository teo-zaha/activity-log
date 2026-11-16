use std::collections::BTreeMap;

use anyhow::{Result, anyhow};
use reqwest::Url;

use super::{ConfigSource, ProviderConfigKey, ProviderConfigShape, ProviderConfigValueType};

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

    /// The items of a list value, or `None` when the key is unset or blank. A stored empty list
    /// (`[]`) is `Some` and empty, so "none" can be told apart from "use the default".
    pub fn list(&self, key: ProviderConfigKey) -> Option<Vec<String>> {
        self.get(key)
            .filter(|value| !value.trim().is_empty())
            .map(parse_config_list)
    }

    /// The lists of a map value by name, or `None` when the key is unset, blank or not a map.
    /// A stored empty map (`{}`) is `Some` and empty, like an empty list.
    pub fn map(&self, key: ProviderConfigKey) -> Option<ConfigMap> {
        self.get(key)
            .filter(|value| !value.trim().is_empty())
            .and_then(parse_config_map)
    }

    pub fn missing_required_keys<'a>(&self, requirements: &'a [ProviderConfigKey]) -> Vec<&'a str> {
        requirements
            .iter()
            .filter(|key| key.required && !self.values.contains_key(key.name))
            .map(|key| key.name)
            .collect()
    }

    /// Stable hash of the non-secret values, used to keep caches of different accounts apart.
    /// Passwords and tokens are left out so they never influence file names on disk.
    pub fn fingerprint(&self, requirements: &[ProviderConfigKey]) -> u64 {
        // FNV-1a: stable across Rust versions, unlike `DefaultHasher`.
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for key in requirements
            .iter()
            .filter(|key| key.value_type() != ProviderConfigValueType::Password)
        {
            let value = self.values.get(key.name).map(String::as_str).unwrap_or("");
            for byte in key
                .name
                .bytes()
                .chain(*b"=")
                .chain(value.bytes())
                .chain(*b"\n")
            {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        hash
    }

    pub fn first_invalid_key<'a>(
        &self,
        requirements: &'a [ProviderConfigKey],
    ) -> Option<&'a ProviderConfigKey> {
        requirements.iter().find(|key| {
            self.values
                .get(key.name)
                .is_some_and(|value| !provider_config_key_accepts(**key, value))
        })
    }
}

/// Whether `value` is valid for `key`: for a list, every item must match the value type.
pub fn provider_config_key_accepts(key: ProviderConfigKey, value: &str) -> bool {
    match key.shape {
        ProviderConfigShape::Single(value_type) => {
            provider_config_value_matches_type(value, value_type)
        }
        ProviderConfigShape::List(item_type) => parse_config_list(value)
            .iter()
            .all(|item| provider_config_value_matches_type(item, item_type)),
        ProviderConfigShape::Map(item_type) => parse_config_map(value).is_some_and(|map| {
            map.values()
                .flatten()
                .all(|item| provider_config_value_matches_type(item, item_type))
        }),
    }
}

pub fn provider_config_value_matches_type(
    value: &str,
    value_type: ProviderConfigValueType,
) -> bool {
    match value_type {
        ProviderConfigValueType::String | ProviderConfigValueType::Password => true,
        ProviderConfigValueType::Bool => parse_bool(value).is_some(),
        ProviderConfigValueType::Int => value.parse::<i64>().is_ok(),
        ProviderConfigValueType::Email => looks_like_email(value),
        ProviderConfigValueType::Url => Url::parse(value).is_ok(),
    }
}

/// Reads a list value: a JSON array of strings, or else a comma-separated list, cleaned by
/// [`clean_config_list`] (quotes come from Windows' "Copy as path").
pub fn parse_config_list(value: &str) -> Vec<String> {
    let items = serde_json::from_str::<Vec<String>>(value)
        .unwrap_or_else(|_| value.split(',').map(str::to_string).collect());
    clean_config_list(&items)
}

/// Trims items, including surrounding quotes, and drops empty ones.
pub fn clean_config_list(items: &[String]) -> Vec<String> {
    items
        .iter()
        .map(|item| item.trim().trim_matches('"').trim())
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect()
}

/// The stored form of a list value, read back by [`parse_config_list`].
pub fn encode_config_list(items: &[String]) -> String {
    serde_json::to_string(items).expect("a list of strings always serializes")
}

/// Lists by name, the value of a [`ProviderConfigShape::Map`] key.
pub type ConfigMap = BTreeMap<String, Vec<String>>;

/// Reads a map value, a JSON object of string arrays, cleaned by [`clean_config_map`]. There
/// is no plain-text form: `None` means it is not such an object.
pub fn parse_config_map(value: &str) -> Option<ConfigMap> {
    serde_json::from_str::<ConfigMap>(value)
        .ok()
        .map(|map| clean_config_map(&map))
}

/// Trims names and items like [`clean_config_list`]; entries without a name are dropped.
pub fn clean_config_map(map: &ConfigMap) -> ConfigMap {
    map.iter()
        .map(|(name, items)| (name.trim().to_string(), clean_config_list(items)))
        .filter(|(name, _)| !name.is_empty())
        .collect()
}

/// The stored form of a map value, read back by [`parse_config_map`].
pub fn encode_config_map(map: &ConfigMap) -> String {
    serde_json::to_string(map).expect("lists of strings by name always serialize")
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" => Some(true),
        "false" => Some(false),
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
