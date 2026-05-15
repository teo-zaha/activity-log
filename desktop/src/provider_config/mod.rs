use activity_log_core::{
    model::{
        ProviderConfigFieldStateDto, ProviderConfigProviderStateDto, ProviderConfigSaveRequestDto,
        ProviderConfigSnapshotDto, Source,
    },
    provider::{
        git::{GIT_DIRS, default_git_dirs_value},
        provider_config::{
            ConfigSource, ProviderConfigKey, ProviderConfigValueType,
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
            _ => {
                return Err(format!(
                    "Stored provider config key '{key}' has unsupported JSON type"
                ));
            }
        };

        values.insert(key.clone(), parsed);
    }

    Ok(values)
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
            let serialized = match value {
                Some(string) => JsonValue::String(string.clone()),
                None => JsonValue::Null,
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

        let next_value = value.value.as_ref().map(|raw| raw.trim().to_string());
        let is_clear = next_value
            .as_ref()
            .is_none_or(|candidate| candidate.is_empty());

        if is_clear {
            store_values.insert(value.key.clone(), None);
            continue;
        }

        let candidate = next_value.expect("checked clear branch");
        if !provider_config_value_matches_type(&candidate, requirement.value_type) {
            return Err(format!(
                "Invalid value for '{}': expected {}",
                value.key, requirement.value_type
            ));
        }

        store_values.insert(value.key.clone(), Some(candidate));
    }

    Ok(())
}

fn exposed_provider_requirements_for(source: Source) -> Result<Vec<ProviderConfigKey>, String> {
    let adapter = provider_registry()
        .into_iter()
        .find(|provider| provider.id() == source)
        .ok_or_else(|| format!("Unknown provider '{source}'"))?;

    Ok(adapter.config_requirements().to_vec())
}

fn display_provider_config_value(
    config_source: &dyn ConfigSource,
    source: &Source,
    requirement: ProviderConfigKey,
) -> Option<String> {
    let resolved = config_source.get(requirement.name);
    if resolved.is_some() {
        return resolved;
    }

    match (source, requirement.name) {
        (Source::Git, key) if key == GIT_DIRS.name() => default_git_dirs_value(),
        _ => None,
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
                .map(|requirement| {
                    let configured = config_source.get(requirement.name);
                    let resolved =
                        display_provider_config_value(config_source, &source, *requirement);
                    let secret = requirement.value_type == ProviderConfigValueType::Password;
                    ProviderConfigFieldStateDto {
                        key: requirement.name.to_string(),
                        label: provider_config_key_label(requirement.name),
                        required: requirement.required,
                        value_type: requirement.value_type.into(),
                        secret,
                        is_set: configured.is_some(),
                        value: if secret { None } else { resolved },
                    }
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
