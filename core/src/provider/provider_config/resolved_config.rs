use std::collections::BTreeMap;

use anyhow::{Result, anyhow};
use reqwest::Url;

use super::{ConfigSource, ProviderConfigKey, ProviderConfigShape, ProviderConfigValueType};

#[derive(Clone, Default)]
pub struct ProviderConfig {
    values: BTreeMap<String, String>,
}

impl std::fmt::Debug for ProviderConfig {
    // Only the keys: the values hold tokens.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderConfig")
            .field("keys", &self.values.keys().collect::<Vec<_>>())
            .finish()
    }
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

    pub fn missing_required_keys<'a>(&self, requirements: &'a [ProviderConfigKey]) -> Vec<&'a str> {
        requirements
            .iter()
            .filter(|key| key.required && !self.values.contains_key(key.name))
            .map(|key| key.name)
            .collect()
    }

    /// Stable hash of the non-secret values, used to keep caches of different accounts apart.
    /// Secrets are left out so they never influence file names on disk; URLs count
    /// as their endpoint (see [`canonical_endpoint`]). `Err` names a URL setting that holds
    /// credentials, a query or a fragment: such a connection is not cached.
    pub fn fingerprint(&self, requirements: &[ProviderConfigKey]) -> Result<u64, &'static str> {
        let mut text = String::new();
        for key in requirements.iter().filter(|key| !key.is_secret()) {
            let value = self.values.get(key.name).map(String::as_str).unwrap_or("");
            let value = match key.shape {
                ProviderConfigShape::Single(ProviderConfigValueType::Url) => {
                    canonical_endpoint(value).ok_or(key.name)?
                }
                ProviderConfigShape::List(ProviderConfigValueType::Url) => parse_config_list(value)
                    .iter()
                    .map(|item| canonical_endpoint(item).ok_or(key.name))
                    .collect::<Result<Vec<_>, _>>()?
                    .join("\n"),
                _ => value.to_string(),
            };
            text.push_str(&format!("{}={value}\n", key.name));
        }
        Ok(crate::utils::fnv1a(text.into_bytes()))
    }

    pub fn first_config_problem(&self, requirements: &[ProviderConfigKey]) -> Option<String> {
        requirements.iter().find_map(|&key| {
            let value = self.values.get(key.name)?;
            let problem = key_problem(key, value)?;
            Some(match key.check {
                Some(_) => problem.to_owned(),
                None => format!("{} must be a {}", key.name, key.shape),
            })
        })
    }
}

/// A URL as the endpoint it names: scheme, lowercase host, port, and the path without trailing
/// slashes (a context path such as `/jira` kept). `None` for a URL with a user name, password,
/// query or fragment. A value that is no URL is kept as written; it is never collected.
fn canonical_endpoint(value: &str) -> Option<String> {
    let Ok(url) = Url::parse(value.trim()) else {
        return Some(value.to_string());
    };
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return None;
    }
    let host = url.host_str().unwrap_or_default().to_lowercase();
    let port = url
        .port()
        .map(|port| format!(":{port}"))
        .unwrap_or_default();
    let path = url.path().trim_end_matches('/');
    Some(format!("{}://{host}{port}{path}", url.scheme()))
}

/// A URL setting for `Debug` output: its endpoint, without user info, query and fragment,
/// which can hold credentials.
pub fn url_for_debug(value: &str) -> String {
    match Url::parse(value.trim()) {
        Ok(mut url) => {
            let _ = url.set_username("");
            let _ = url.set_password(None);
            url.set_query(None);
            url.set_fragment(None);
            url.to_string()
        }
        Err(_) => "<not a URL>".to_string(),
    }
}

/// Whether `value` is valid for `key`: for a list, every item must match the value type.
pub fn provider_config_key_accepts(key: ProviderConfigKey, value: &str) -> bool {
    key_problem(key, value).is_none()
}

fn key_problem(key: ProviderConfigKey, value: &str) -> Option<&'static str> {
    match key.shape {
        ProviderConfigShape::Single(_) => value_problem(value, key),
        ProviderConfigShape::List(_) => parse_config_list(value)
            .iter()
            .find_map(|item| value_problem(item, key)),
    }
}

pub fn provider_config_value_matches_type(
    value: &str,
    value_type: ProviderConfigValueType,
) -> bool {
    type_problem(value, value_type).is_none()
}

/// Why `value` is invalid for the given key, as the settings say it. The
/// one rule for every value read or written (B64: the settings and the file agree). A URL with
/// a user name or password is a URL here: a file that holds one still loads, and its check says
/// what to change; only a new value is refused for it ([`url_with_credentials`]).
pub fn value_problem(value: &str, key: ProviderConfigKey) -> Option<&'static str> {
    type_problem(value, key.value_type())
        .or_else(|| key.check.and_then(|check| check.problem(value)))
}

fn type_problem(value: &str, value_type: ProviderConfigValueType) -> Option<&'static str> {
    let unless = |fits: bool, message| (!fits).then_some(message);
    match value_type {
        ProviderConfigValueType::String | ProviderConfigValueType::Password => None,
        ProviderConfigValueType::Bool => {
            unless(parse_bool(value).is_some(), "Enter true or false.")
        }
        ProviderConfigValueType::Int => {
            unless(value.trim().parse::<i64>().is_ok(), "Enter a whole number.")
        }
        ProviderConfigValueType::Email => {
            unless(looks_like_email(value), "Enter an email address.")
        }
        ProviderConfigValueType::Url => unless(
            Url::parse(value.trim()).is_ok_and(|url| {
                matches!(url.scheme(), "http" | "https")
                    && url.host_str().is_some_and(|host| !host.is_empty())
            }),
            "Enter an http or https address.",
        ),
    }
}

/// Whether a URL value names a user or a password, which belong in the token field.
pub fn url_with_credentials(value: &str) -> bool {
    Url::parse(value.trim()).is_ok_and(|url| !url.username().is_empty() || url.password().is_some())
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

/// A bool as every reader of the config reads it: `true` or `false`, any case, trimmed.
pub(crate) fn parse_bool(value: &str) -> Option<bool> {
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
