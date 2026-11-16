use activity_log_core::{
    model::{
        ProviderConfigFieldStateDto, ProviderConfigFieldValueDto, ProviderConfigMapEntryDto,
        ProviderConfigProviderStateDto, ProviderConfigSaveRequestDto, ProviderConfigSnapshotDto,
        ProviderConfigValueDto, Source,
    },
    provider::{
        git::{GIT_DIRS, default_git_root},
        provider_config::{
            ConfigMap, ConfigSource, ProviderConfigKey, ProviderConfigShape,
            ProviderConfigValueType, clean_config_list, clean_config_map, encode_config_list,
            encode_config_map, parse_config_list, parse_config_map, provider_config_key_accepts,
            provider_config_value_matches_type,
        },
        provider_registry,
    },
};
use serde_json::{Map as JsonMap, Value as JsonValue};
use std::collections::BTreeMap;
use tauri_plugin_store::StoreExt;

const CONFIG_STORE_PATH: &str = "settings.json";
const CONFIG_STORE_KEY: &str = "providerConfig";

#[derive(Debug, Clone, Default)]
pub(crate) struct StoreEnvConfigSource {
    // `None` is an explicit unset override that shadows process env fallback.
    pub(crate) store_values: BTreeMap<String, Option<String>>,
}

impl ConfigSource for StoreEnvConfigSource {
    fn get(&self, key: &str) -> Option<String> {
        if let Some(store_value) = self.store_values.get(key) {
            return store_value.clone();
        }

        std::env::var(key).ok()
    }
}

pub(crate) fn load_store_env_config_source(
    app: &tauri::AppHandle,
) -> Result<StoreEnvConfigSource, String> {
    Ok(StoreEnvConfigSource {
        store_values: read_store_provider_config(app)?,
    })
}

pub(crate) fn read_store_provider_config(
    app: &tauri::AppHandle,
) -> Result<BTreeMap<String, Option<String>>, String> {
    let store = app
        .store(CONFIG_STORE_PATH)
        .map_err(|err| format!("Failed to open config store: {err}"))?;

    let Some(raw) = store.get(CONFIG_STORE_KEY) else {
        return Ok(BTreeMap::new());
    };

    let Some(object) = raw.as_object() else {
        return Err("Stored provider config must be a JSON object".to_string());
    };

    let mut values = BTreeMap::new();
    for (key, value) in object {
        let parsed = match value {
            JsonValue::Null => None,
            JsonValue::String(string) => Some(string.clone()),
            JsonValue::Bool(boolean) => Some(boolean.to_string()),
            JsonValue::Number(number) => Some(number.to_string()),
            JsonValue::Array(items) => Some(encode_config_list(&string_items(items).ok_or_else(
                || format!("Stored provider config key '{key}' must be a list of strings"),
            )?)),
            JsonValue::Object(entries) => {
                let map = entries
                    .iter()
                    .map(|(name, items)| {
                        let items = items.as_array().and_then(|items| string_items(items))?;
                        Some((name.clone(), items))
                    })
                    .collect::<Option<ConfigMap>>()
                    .ok_or_else(|| {
                        format!(
                            "Stored provider config key '{key}' must map names to lists of strings"
                        )
                    })?;
                Some(encode_config_map(&map))
            }
        };

        values.insert(key.clone(), parsed);
    }

    Ok(values)
}

fn string_items(items: &[JsonValue]) -> Option<Vec<String>> {
    items
        .iter()
        .map(|item| item.as_str().map(str::to_string))
        .collect()
}

pub(crate) fn write_store_provider_config(
    app: &tauri::AppHandle,
    values: &BTreeMap<String, Option<String>>,
) -> Result<(), String> {
    let store = app
        .store(CONFIG_STORE_PATH)
        .map_err(|err| format!("Failed to open config store: {err}"))?;

    let json_values = values
        .iter()
        .map(|(key, value)| {
            // Lists and maps are kept as JSON, so the file stays readable and editable.
            let serialized = match (value, shape_of(key)) {
                (None, _) => JsonValue::Null,
                (Some(raw), Some(ProviderConfigShape::List(_))) => {
                    JsonValue::from(parse_config_list(raw))
                }
                (Some(raw), Some(ProviderConfigShape::Map(_))) => parse_config_map(raw)
                    .map_or_else(|| JsonValue::String(raw.clone()), JsonValue::from_iter),
                (Some(string), _) => JsonValue::String(string.clone()),
            };
            (key.clone(), serialized)
        })
        .collect::<JsonMap<String, JsonValue>>();

    store.set(CONFIG_STORE_KEY, JsonValue::Object(json_values));
    store
        .save()
        .map_err(|err| format!("Failed to save config store: {err}"))
}

pub(crate) fn apply_provider_config_updates(
    store_values: &mut BTreeMap<String, Option<String>>,
    request: &ProviderConfigSaveRequestDto,
) -> Result<(), String> {
    let requirements = exposed_provider_requirements_for(request.source.clone())?;
    let requirement_by_key = requirements
        .iter()
        .map(|requirement| (requirement.name, requirement))
        .collect::<BTreeMap<_, _>>();

    for value in &request.values {
        let requirement = requirement_by_key.get(value.key.as_str()).ok_or_else(|| {
            format!(
                "Unknown config key '{}' for provider '{}'.",
                value.key, request.source
            )
        })?;

        let invalid = |expected: String| format!("Invalid value for '{}': {expected}", value.key);
        let next = match (&value.value, requirement.shape) {
            (None, _) => None,
            // An empty list or map is kept: it means "none", while clearing means "use the
            // default".
            (Some(ProviderConfigValueDto::List(items)), ProviderConfigShape::List(value_type)) => {
                let items = clean_config_list(items);
                check_items(&value.key, &items, value_type)?;
                Some(encode_config_list(&items))
            }
            (Some(ProviderConfigValueDto::Map(map)), ProviderConfigShape::Map(value_type)) => {
                let map = clean_config_map(map);
                check_items(&value.key, map.values().flatten(), value_type)?;
                Some(encode_config_map(&map))
            }
            (Some(ProviderConfigValueDto::Text(raw)), ProviderConfigShape::List(value_type)) => {
                if !provider_config_key_accepts(**requirement, raw) {
                    return Err(invalid(format!("expected a list of {value_type}")));
                }
                Some(encode_config_list(&parse_config_list(raw)))
            }
            (Some(ProviderConfigValueDto::Text(raw)), ProviderConfigShape::Map(_)) => {
                match parse_config_map(raw) {
                    Some(map) if provider_config_key_accepts(**requirement, raw) => {
                        Some(encode_config_map(&map))
                    }
                    _ => return Err(invalid(format!("expected {}", requirement.shape))),
                }
            }
            (Some(ProviderConfigValueDto::Text(raw)), ProviderConfigShape::Single(value_type)) => {
                let candidate = raw.trim();
                if candidate.is_empty() {
                    None
                } else if provider_config_value_matches_type(candidate, value_type) {
                    Some(candidate.to_string())
                } else {
                    return Err(invalid(format!("expected {value_type}")));
                }
            }
            (Some(ProviderConfigValueDto::List(_)), shape) => {
                return Err(invalid(format!("expected {shape}, not a list")));
            }
            (Some(ProviderConfigValueDto::Map(_)), shape) => {
                return Err(invalid(format!("expected {shape}, not lists by name")));
            }
        };

        store_values.insert(value.key.clone(), next);
    }

    Ok(())
}

fn check_items<'a>(
    key: &str,
    items: impl IntoIterator<Item = &'a String>,
    value_type: ProviderConfigValueType,
) -> Result<(), String> {
    match items
        .into_iter()
        .find(|item| !provider_config_value_matches_type(item, value_type))
    {
        Some(bad) => Err(format!(
            "Invalid item \"{bad}\" for '{key}': expected {value_type}"
        )),
        None => Ok(()),
    }
}

fn exposed_provider_requirements_for(source: Source) -> Result<Vec<ProviderConfigKey>, String> {
    let adapter = provider_registry()
        .into_iter()
        .find(|provider| provider.id() == source)
        .ok_or_else(|| format!("Unknown provider '{source}'"))?;

    Ok(adapter.config_requirements().to_vec())
}

fn shape_of(key: &str) -> Option<ProviderConfigShape> {
    provider_registry().into_iter().find_map(|provider| {
        provider
            .config_requirements()
            .iter()
            .find(|requirement| requirement.name == key)
            .map(|requirement| requirement.shape)
    })
}

/// The configured value with its type. An unset list or map shows the default the provider falls
/// back to, so the settings show what is actually used; a secret is never sent.
fn field_value(
    config_source: &dyn ConfigSource,
    source: &Source,
    requirement: ProviderConfigKey,
) -> ProviderConfigFieldValueDto {
    let configured = config_source
        .get(requirement.name)
        .filter(|raw| !raw.trim().is_empty());
    match requirement.shape {
        ProviderConfigShape::Single(value_type) => {
            let secret = value_type == ProviderConfigValueType::Password;
            ProviderConfigFieldValueDto::Single {
                value_type: value_type.into(),
                secret,
                value: configured.filter(|_| !secret),
            }
        }
        ProviderConfigShape::List(item_type) => ProviderConfigFieldValueDto::List {
            item_type: item_type.into(),
            items: match configured {
                Some(raw) => parse_config_list(&raw),
                None => default_list(source, requirement.name),
            },
        },
        ProviderConfigShape::Map(item_type) => ProviderConfigFieldValueDto::Map {
            item_type: item_type.into(),
            entries: configured
                .and_then(|raw| parse_config_map(&raw))
                .unwrap_or_else(|| default_map(source, requirement.name))
                .into_iter()
                .map(|(name, items)| ProviderConfigMapEntryDto { name, items })
                .collect(),
            names: map_names(source, requirement.name),
        },
    }
}

fn default_list(source: &Source, key: &str) -> Vec<String> {
    match (source, key) {
        (Source::Git, key) if key == GIT_DIRS.name() => default_git_root()
            .map(|root| vec![root.display().to_string()])
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

#[cfg_attr(not(target_os = "windows"), allow(unused_variables))]
fn default_map(source: &Source, key: &str) -> ConfigMap {
    match (source, key) {
        #[cfg(target_os = "windows")]
        (Source::Git, key)
            if key == activity_log_core::provider::git::windows::WSL_GIT_DIRS.name() =>
        {
            activity_log_core::provider::git::windows::default_wsl_dirs()
        }
        _ => ConfigMap::new(),
    }
}

/// The names a map field can hold, offered when adding one.
#[cfg_attr(not(target_os = "windows"), allow(unused_variables))]
fn map_names(source: &Source, key: &str) -> Vec<String> {
    match (source, key) {
        #[cfg(target_os = "windows")]
        (Source::Git, key)
            if key == activity_log_core::provider::git::windows::WSL_GIT_DIRS.name() =>
        {
            activity_log_core::provider::git::windows::default_wsl_distros()
        }
        _ => Vec::new(),
    }
}

pub(crate) fn build_provider_config_snapshot(
    config_source: &dyn ConfigSource,
) -> ProviderConfigSnapshotDto {
    let providers = provider_registry()
        .into_iter()
        .map(|adapter| {
            let source = adapter.id();
            let fields = adapter
                .config_requirements()
                .iter()
                .map(|requirement| ProviderConfigFieldStateDto {
                    key: requirement.name.to_string(),
                    label: provider_config_key_label(requirement.name),
                    required: requirement.required,
                    is_set: config_source.get(requirement.name).is_some(),
                    value: field_value(config_source, &source, *requirement),
                })
                .collect();

            ProviderConfigProviderStateDto { source, fields }
        })
        .collect();

    ProviderConfigSnapshotDto { providers }
}

fn provider_config_key_label(key: &str) -> String {
    key.split('_')
        .filter(|part| !part.is_empty())
        .map(label_part)
        .collect::<Vec<_>>()
        .join(" ")
}

fn label_part(raw: &str) -> String {
    match raw {
        "API" | "URL" | "HTTP" | "HTTPS" | "TLS" | "SSL" | "ID" | "OS" | "PAT" | "JWT" => {
            raw.to_string()
        }
        "GITLAB" => "GitLab".to_string(),
        "JIRA" => "Jira".to_string(),
        "GIT" => "Git".to_string(),
        "OUTLOOK" => "Outlook".to_string(),
        "SYSTEM" => "System".to_string(),
        _ => {
            let mut chars = raw.chars();
            let Some(first) = chars.next() else {
                return String::new();
            };
            let rest = chars.as_str().to_ascii_lowercase();
            format!("{}{}", first.to_ascii_uppercase(), rest)
        }
    }
}

#[cfg(test)]
mod tests;
