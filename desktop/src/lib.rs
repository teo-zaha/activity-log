use activity_log_core::{
    model::{
        CollectRequest, CollectingResponse, ProviderConfigFieldStateDto,
        ProviderConfigProviderStateDto, ProviderConfigSaveRequestDto,
        ProviderConfigSaveResponseDto, ProviderConfigSnapshotDto, ProviderConfigValueTypeDto,
        Source,
    },
    provider::{
        provider_config::{
            ConfigSource, ProviderConfigKey, ProviderConfigValueType,
            provider_config_value_matches_type,
        },
        provider_registry,
    },
    runtime::{ActivitySnapshot, CollectionResult, CollectionRuntime, resolve_collect_options},
};
use rfd::FileDialog;
use serde::Deserialize;
use serde_json::{Map as JsonMap, Value as JsonValue};
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};
use tauri::Manager;
use tauri_plugin_store::StoreExt;
use tokio::sync::RwLock;

const CONFIG_STORE_PATH: &str = "settings.json";
const CONFIG_STORE_KEY: &str = "providerConfig";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ExportFormat {
    Json,
    Markdown,
}

impl ExportFormat {
    fn extension(&self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Markdown => "md",
        }
    }

    fn filter_name(&self) -> &'static str {
        match self {
            Self::Json => "JSON",
            Self::Markdown => "Markdown",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportRequest {
    content: String,
    file_name: String,
    format: ExportFormat,
}

#[derive(Clone)]
struct AppState {
    runtime: Arc<RwLock<CollectionRuntime>>,
}

#[derive(Debug, Clone, Default)]
struct StoreEnvConfigSource {
    // `None` is an explicit unset override that shadows process env fallback.
    store_values: BTreeMap<String, Option<String>>,
}

impl ConfigSource for StoreEnvConfigSource {
    fn get(&self, key: &str) -> Option<String> {
        if let Some(store_value) = self.store_values.get(key) {
            return store_value.clone();
        }

        std::env::var(key).ok()
    }
}

#[tauri::command]
async fn get_activity(state: tauri::State<'_, AppState>) -> Result<serde_json::Value, String> {
    let runtime = state.runtime.read().await.clone();

    match runtime.activity_snapshot().await {
        ActivitySnapshot::Collecting => {
            Ok(serde_json::to_value(CollectingResponse { collecting: true }).unwrap())
        }
        ActivitySnapshot::Ready(log) => Ok(serde_json::to_value(log).unwrap()),
        ActivitySnapshot::Empty => Err("No data yet".to_string()),
    }
}

#[tauri::command]
async fn get_providers(
    state: tauri::State<'_, AppState>,
) -> Result<activity_log_core::model::ProvidersResponse, String> {
    Ok(state.runtime.read().await.providers_response())
}

#[tauri::command]
async fn collect(
    state: tauri::State<'_, AppState>,
    request: CollectRequest,
) -> Result<serde_json::Value, String> {
    let runtime = state.runtime.read().await.clone();
    let options = resolve_collect_options(&request).map_err(str::to_string)?;

    match runtime.collect_with_options(options).await {
        Ok(CollectionResult::Ready(log)) => Ok(serde_json::to_value(log).unwrap()),
        Ok(CollectionResult::Collecting) => {
            Ok(serde_json::to_value(CollectingResponse { collecting: true }).unwrap())
        }
        Err(err) => Err(format!("Collection failed: {err}")),
    }
}

#[tauri::command]
fn get_provider_config(app: tauri::AppHandle) -> Result<ProviderConfigSnapshotDto, String> {
    let config_source = load_store_env_config_source(&app)?;
    Ok(build_provider_config_snapshot(&config_source))
}

#[tauri::command]
async fn save_provider_config(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    request: ProviderConfigSaveRequestDto,
) -> Result<ProviderConfigSaveResponseDto, String> {
    let mut store_values = read_store_provider_config(&app)?;
    apply_provider_config_updates(&mut store_values, &request)?;
    write_store_provider_config(&app, &store_values)?;

    let config_source = StoreEnvConfigSource {
        store_values: store_values.clone(),
    };
    let runtime = CollectionRuntime::build_with_config_source(&config_source)
        .await
        .map_err(|err| format!("Failed to refresh runtime config: {err}"))?;
    let providers_response = runtime.providers_response();

    {
        let mut runtime_slot = state.runtime.write().await;
        *runtime_slot = runtime;
    }

    let snapshot = build_provider_config_snapshot(&config_source);
    Ok(ProviderConfigSaveResponseDto {
        snapshot,
        providers_response,
    })
}

#[tauri::command]
fn save_export(request: ExportRequest) -> Result<(), String> {
    let selected_path = FileDialog::new()
        .set_title("Export activity log")
        .set_file_name(&request.file_name)
        .add_filter(request.format.filter_name(), &[request.format.extension()])
        .save_file();

    let Some(path) = selected_path else {
        return Ok(());
    };

    let path = ensure_extension(path, request.format.extension());
    std::fs::write(&path, request.content).map_err(|err| format!("Failed to save export: {err}"))
}

fn ensure_extension(mut path: PathBuf, extension: &str) -> PathBuf {
    let has_expected_extension = path
        .extension()
        .and_then(|current| current.to_str())
        .is_some_and(|current| current.eq_ignore_ascii_case(extension));

    if !has_expected_extension {
        path.set_extension(extension);
    }

    path
}

fn load_store_env_config_source(app: &tauri::AppHandle) -> Result<StoreEnvConfigSource, String> {
    Ok(StoreEnvConfigSource {
        store_values: read_store_provider_config(app)?,
    })
}

fn read_store_provider_config(
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

fn write_store_provider_config(
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

fn apply_provider_config_updates(
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

    Ok(adapter
        .config_requirements()
        .iter()
        .copied()
        .filter(|requirement| is_exposed_provider_config_key(&source, requirement.name))
        .collect())
}

fn is_exposed_provider_config_key(source: &Source, key: &str) -> bool {
    !matches!(
        (source, key),
        (Source::Jira, "JIRA_AUTH_TYPE" | "JIRA_INSECURE")
    )
}

fn build_provider_config_snapshot(config_source: &dyn ConfigSource) -> ProviderConfigSnapshotDto {
    let providers = provider_registry()
        .into_iter()
        .map(|adapter| {
            let source = adapter.id();
            let fields = adapter
                .config_requirements()
                .iter()
                .filter(|requirement| is_exposed_provider_config_key(&source, requirement.name))
                .map(|requirement| {
                    let resolved = config_source.get(requirement.name);
                    let secret = requirement.value_type == ProviderConfigValueType::Password;
                    ProviderConfigFieldStateDto {
                        key: requirement.name.to_string(),
                        label: provider_config_key_label(requirement.name),
                        required: requirement.required,
                        value_type: provider_config_value_type_dto(requirement.value_type),
                        secret,
                        is_set: resolved.is_some(),
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

fn provider_config_value_type_dto(
    value_type: ProviderConfigValueType,
) -> ProviderConfigValueTypeDto {
    match value_type {
        ProviderConfigValueType::String => ProviderConfigValueTypeDto::String,
        ProviderConfigValueType::Bool => ProviderConfigValueTypeDto::Bool,
        ProviderConfigValueType::Int => ProviderConfigValueTypeDto::Int,
        ProviderConfigValueType::Password => ProviderConfigValueTypeDto::Password,
        ProviderConfigValueType::Email => ProviderConfigValueTypeDto::Email,
        ProviderConfigValueType::Url => ProviderConfigValueTypeDto::Url,
    }
}

pub fn run() {
    let runtime =
        tauri::async_runtime::block_on(CollectionRuntime::build_default()).expect("app runtime");
    let app_state = AppState {
        runtime: Arc::new(RwLock::new(runtime)),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::new().build())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            get_activity,
            get_providers,
            collect,
            get_provider_config,
            save_provider_config,
            save_export
        ])
        .setup(move |app| {
            let app_handle = app.handle().clone();
            let source = load_store_env_config_source(&app_handle)
                .map_err(|err| -> Box<dyn std::error::Error> { err.into() })?;
            let runtime = tauri::async_runtime::block_on(
                CollectionRuntime::build_with_config_source(&source),
            )?;

            tauri::async_runtime::block_on(async {
                let state: tauri::State<'_, AppState> = app_handle.state();
                *state.runtime.write().await = runtime.clone();
            });

            if let Some(task) = runtime.initial_collection_task() {
                tauri::async_runtime::spawn(task);
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use activity_log_core::{
        model::{
            ProviderConfigSaveRequestDto, ProviderConfigSaveValueDto, ProviderConfigValueTypeDto,
            Source,
        },
        provider::provider_config::ProviderConfigValueType,
    };

    use super::{
        StoreEnvConfigSource, apply_provider_config_updates, build_provider_config_snapshot,
        provider_config_value_type_dto,
    };

    #[test]
    fn maps_provider_config_value_types_to_public_dto() {
        assert_eq!(
            provider_config_value_type_dto(ProviderConfigValueType::String),
            ProviderConfigValueTypeDto::String
        );
        assert_eq!(
            provider_config_value_type_dto(ProviderConfigValueType::Bool),
            ProviderConfigValueTypeDto::Bool
        );
        assert_eq!(
            provider_config_value_type_dto(ProviderConfigValueType::Int),
            ProviderConfigValueTypeDto::Int
        );
        assert_eq!(
            provider_config_value_type_dto(ProviderConfigValueType::Password),
            ProviderConfigValueTypeDto::Password
        );
        assert_eq!(
            provider_config_value_type_dto(ProviderConfigValueType::Email),
            ProviderConfigValueTypeDto::Email
        );
        assert_eq!(
            provider_config_value_type_dto(ProviderConfigValueType::Url),
            ProviderConfigValueTypeDto::Url
        );
    }

    #[test]
    fn save_updates_clear_values_with_null_tombstone() {
        let mut store_values = std::collections::BTreeMap::new();
        store_values.insert(
            "JIRA_URL".to_string(),
            Some("https://jira.example".to_string()),
        );

        let request = ProviderConfigSaveRequestDto {
            source: Source::Jira,
            values: vec![ProviderConfigSaveValueDto {
                key: "JIRA_URL".to_string(),
                value: None,
            }],
        };

        apply_provider_config_updates(&mut store_values, &request).expect("valid request");

        assert_eq!(store_values.get("JIRA_URL"), Some(&None));
    }

    #[test]
    fn snapshot_masks_password_values() {
        let mut store_values = std::collections::BTreeMap::new();
        store_values.insert(
            "JIRA_TOKEN".to_string(),
            Some("super-secret-token".to_string()),
        );

        let snapshot = build_provider_config_snapshot(&StoreEnvConfigSource { store_values });
        let jira = snapshot
            .providers
            .iter()
            .find(|provider| provider.source == Source::Jira)
            .expect("jira provider");
        let token = jira
            .fields
            .iter()
            .find(|field| field.key == "JIRA_TOKEN")
            .expect("jira token field");

        assert!(token.secret);
        assert!(token.is_set);
        assert_eq!(token.value, None);
        assert_eq!(token.label, "Jira Token");
    }

    #[test]
    fn label_formatter_keeps_common_acronyms_readable() {
        assert_eq!(super::provider_config_key_label("JIRA_URL"), "Jira URL");
        assert_eq!(
            super::provider_config_key_label("GITLAB_TOKEN"),
            "GitLab Token"
        );
        assert_eq!(
            super::provider_config_key_label("JIRA_INSECURE"),
            "Jira Insecure"
        );
    }

    #[test]
    fn snapshot_hides_non_user_jira_fields() {
        let snapshot = build_provider_config_snapshot(&StoreEnvConfigSource::default());
        let jira = snapshot
            .providers
            .iter()
            .find(|provider| provider.source == Source::Jira)
            .expect("jira provider");

        assert!(
            jira.fields
                .iter()
                .all(|field| field.key != "JIRA_AUTH_TYPE")
        );
        assert!(jira.fields.iter().all(|field| field.key != "JIRA_INSECURE"));
    }

    #[test]
    fn save_rejects_hidden_jira_fields() {
        let mut store_values = std::collections::BTreeMap::new();
        let request = ProviderConfigSaveRequestDto {
            source: Source::Jira,
            values: vec![ProviderConfigSaveValueDto {
                key: "JIRA_AUTH_TYPE".to_string(),
                value: Some("basic".to_string()),
            }],
        };

        let result = apply_provider_config_updates(&mut store_values, &request);
        assert!(result.is_err());
    }
}
