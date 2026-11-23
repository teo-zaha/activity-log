//! Activity Log 0.0.7's own read rules (tag `v0.0.7`, `f0846a0`), ported as they were so the
//! import reads its files as that release did: the desktop's `read_store_provider_config`,
//! `ProviderConfig`'s `list` and `bool`, `parse_config_list`, `encode_config_list`,
//! `parse_bool` and the Windows `parse_wsl_repo`. Today's helpers of the same names may change;
//! these must not. Two departures, both the import's own rules: a blank value is unset (0.0.7
//! sent it on and failed later), and the process environment is never read.

use std::collections::BTreeMap;

use serde_json::Value as JsonValue;

#[cfg(test)]
mod tests;

/// The keys 0.0.7 read, in its own names.
pub(super) const GITLAB_URL: &str = "GITLAB_URL";
pub(super) const GITLAB_TOKEN: &str = "GITLAB_TOKEN";
pub(super) const GITLAB_USER: &str = "GITLAB_USER";
pub(super) const JIRA_URL: &str = "JIRA_URL";
pub(super) const JIRA_TOKEN: &str = "JIRA_TOKEN";
pub(super) const JIRA_INSECURE: &str = "JIRA_INSECURE";
pub(super) const GIT_DIRS: &str = "GIT_DIRS";
pub(super) const WSL_DISTROS: &str = "WSL_DISTROS";
pub(super) const WSL_GIT_DIRS: &str = "WSL_GIT_DIRS";
pub(super) const KEYS: &[&str] = &[
    GITLAB_URL,
    GITLAB_TOKEN,
    GITLAB_USER,
    JIRA_URL,
    JIRA_TOKEN,
    JIRA_INSECURE,
    GIT_DIRS,
    WSL_DISTROS,
    WSL_GIT_DIRS,
];

/// Why `settings.json`'s `providerConfig` was refused as a whole, as 0.0.7 refused it. Names a
/// key, never a value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreRefusal {
    NotAnObject,
    /// An array with an item that is not a string.
    NotAListOfStrings {
        key: String,
    },
    /// An object, or another type 0.0.7 did not take.
    UnsupportedType {
        key: String,
    },
}

impl std::fmt::Display for StoreRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotAnObject => write!(f, "`providerConfig` is not an object"),
            Self::NotAListOfStrings { key } => write!(f, "`{key}` is not a list of strings"),
            Self::UnsupportedType { key } => write!(f, "`{key}` has an unsupported type"),
        }
    }
}

/// `read_store_provider_config`: the store's `providerConfig` value (`None` when the store has
/// no such key) as 0.0.7's desktop read it. `None` in the map is an explicit `null`.
pub(super) fn read_store_provider_config(
    raw: Option<&JsonValue>,
) -> Result<BTreeMap<String, Option<String>>, StoreRefusal> {
    let Some(raw) = raw else {
        return Ok(BTreeMap::new());
    };
    let Some(object) = raw.as_object() else {
        return Err(StoreRefusal::NotAnObject);
    };
    let mut values = BTreeMap::new();
    for (key, value) in object {
        let parsed = match value {
            JsonValue::Null => None,
            JsonValue::String(string) => Some(string.clone()),
            JsonValue::Bool(boolean) => Some(boolean.to_string()),
            JsonValue::Number(number) => Some(number.to_string()),
            JsonValue::Array(items) => {
                let items = items
                    .iter()
                    .map(|item| item.as_str().map(str::to_string))
                    .collect::<Option<Vec<_>>>()
                    .ok_or_else(|| StoreRefusal::NotAListOfStrings { key: key.clone() })?;
                Some(encode_config_list(&items))
            }
            JsonValue::Object(_) => {
                return Err(StoreRefusal::UnsupportedType { key: key.clone() });
            }
        };
        values.insert(key.clone(), parsed);
    }
    Ok(values)
}

/// The values a legacy file gives, read as 0.0.7's `ProviderConfig` read them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct LegacyValues(BTreeMap<String, Option<String>>);

impl LegacyValues {
    pub(super) fn new(values: BTreeMap<String, Option<String>>) -> Self {
        Self(values)
    }

    /// The text of `key`; an explicit `null`, an absent key and a blank value are unset.
    pub(super) fn get(&self, key: &str) -> Option<&str> {
        self.0
            .get(key)
            .and_then(Option::as_deref)
            .filter(|value| !value.trim().is_empty())
    }

    /// `ProviderConfig::list`: `None` while unset or blank, an empty list for "none".
    pub(super) fn list(&self, key: &str) -> Option<Vec<String>> {
        self.get(key).map(parse_config_list)
    }

    /// `ProviderConfig::bool`: anything but `true`/`false` is false.
    pub(super) fn bool(&self, key: &str) -> bool {
        self.get(key).and_then(parse_bool).unwrap_or(false)
    }

    /// Whether any key 0.0.7 read is set by these rules: a file that sets none gives nothing
    /// to prefer it for (Q12).
    pub(super) fn sets_any(&self) -> bool {
        KEYS.iter().any(|key| self.get(key).is_some())
    }

    /// The keys 0.0.7 did not read, by name only.
    pub(super) fn unknown_keys(&self) -> Vec<&str> {
        self.0
            .keys()
            .map(String::as_str)
            .filter(|key| !KEYS.contains(key))
            .collect()
    }
}

/// `parse_config_list`: a JSON array of strings, or else a comma-separated list, cleaned.
pub(super) fn parse_config_list(value: &str) -> Vec<String> {
    let items = serde_json::from_str::<Vec<String>>(value)
        .unwrap_or_else(|_| value.split(',').map(str::to_string).collect());
    clean_config_list(&items)
}

/// `clean_config_list`: items trimmed, surrounding quotes included, and empty ones dropped.
fn clean_config_list(items: &[String]) -> Vec<String> {
    items
        .iter()
        .map(|item| item.trim().trim_matches('"').trim())
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect()
}

/// `encode_config_list`: the stored form of a list, read back by [`parse_config_list`].
pub(super) fn encode_config_list(items: &[String]) -> String {
    serde_json::to_string(items).expect("a list of strings always serializes")
}

/// `parse_bool`: `true` or `false`, any case, trimmed.
pub(super) fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// A repository inside a WSL distribution, `Distro:/absolute/linux/path`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct WslRepo {
    pub(super) distro: String,
    pub(super) path: String,
}

/// `parse_wsl_repo`; a one-letter "distro" is a Windows drive, not WSL.
pub(super) fn parse_wsl_repo(spec: &str) -> Option<WslRepo> {
    match spec.split_once(':') {
        Some((distro, path)) if distro.len() > 1 && path.starts_with('/') => Some(WslRepo {
            distro: distro.to_string(),
            path: path.trim_end_matches('/').to_string(),
        }),
        _ => None,
    }
}
