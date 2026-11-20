//! The settings pane's view of `config.toml`: connections with their fields, and the edits it
//! sends back. The file is shared with the CLI and the API server.

use activity_log_core::{
    model::{
        AddConnectionRequestDto, BackendError, ConnectionId, ProviderConfigConnectionStateDto,
        ProviderConfigFieldStateDto, ProviderConfigFieldValueDto, ProviderConfigSaveRequestDto,
        ProviderConfigSnapshotDto, ProviderConfigValueDto, ProviderKindDto, Source, SupportedOs,
    },
    provider::{
        connection::{Connection, ConnectionLocation, Instances},
        git::{GIT_DIRS, WSL_HOME, default_git_root},
        provider_config::{
            ConfigSource, ProviderConfigKey, ProviderConfigShape, ProviderConfigValueType,
            clean_config_list, encode_config_list, parse_config_list, provider_config_key_accepts,
            provider_config_value_matches_type,
        },
        provider_registry,
    },
    settings::{ConfigEditError, ConfigFile, warn_no_location},
};

/// The settings file, `None` without a settings location.
pub(crate) fn load_config() -> Result<Option<ConfigFile>, BackendError> {
    match ConfigFile::resolve_path(None) {
        Some(path) => Ok(Some(ConfigFile::load(path)?)),
        None => {
            warn_no_location();
            Ok(None)
        }
    }
}

/// The connections the settings pane shows. Its problems were logged when the file was read.
pub(crate) fn shown_connections(file: &ConfigFile) -> Vec<Connection> {
    file.connections().connections
}

/// The pane's edits of one connection, checked against its kind's settings before the file is
/// read: the label, then each value in its stored form (`None` clears it).
pub(crate) struct SaveChanges {
    connection: ConnectionId,
    label: Option<String>,
    values: Vec<(ProviderConfigKey, Option<String>)>,
}

/// The edits of `request`, or why they cannot be saved: a value that does not fit its setting,
/// or a setting the kind does not have.
pub(crate) fn save_changes(
    request: &ProviderConfigSaveRequestDto,
) -> Result<SaveChanges, BackendError> {
    let id = &request.connection;
    let keys = requirements_of(&id.source)?;
    let values = request
        .values
        .iter()
        .map(|value| {
            let key = keys
                .iter()
                .copied()
                .find(|key| key.name == value.key)
                .ok_or_else(|| {
                    BackendError::invalid_request(format!(
                        "Unknown setting '{}' for {id}.",
                        value.key
                    ))
                })?;
            let raw = match &value.value {
                None => None,
                Some(value) => normalize(key, value)?,
            };
            Ok((key, raw))
        })
        .collect::<Result<_, BackendError>>()?;
    Ok(SaveChanges {
        connection: id.clone(),
        label: request.label.clone(),
        values,
    })
}

/// Applies checked edits to the file as read under the config lock.
pub(crate) fn apply_save(
    file: &mut ConfigFile,
    changes: &SaveChanges,
) -> Result<(), ConfigEditError> {
    let id = &changes.connection;
    // Also when nothing changes: a save is no answer for a connection that is gone.
    file.require(id)?;
    if let Some(label) = &changes.label {
        file.rename(id, label)?;
    }
    for (key, raw) in &changes.values {
        file.set(id, *key, raw.as_deref())?;
    }
    Ok(())
}

pub(crate) fn apply_add(
    file: &mut ConfigFile,
    request: &AddConnectionRequestDto,
) -> Result<ConnectionId, ConfigEditError> {
    let location = match &request.wsl_distro {
        Some(distro) => ConnectionLocation::Wsl(distro.clone()),
        None => ConnectionLocation::Local,
    };
    file.add(&request.source, request.label.as_deref(), location)
}

/// The stored form of a value, checked against its key: `None` for an empty single value,
/// which clears it. An empty list is kept: it means "none", while clearing means the default.
fn normalize(
    key: ProviderConfigKey,
    value: &ProviderConfigValueDto,
) -> Result<Option<String>, BackendError> {
    let invalid = |message: String| Err(BackendError::invalid_request(message));
    match (value, key.shape) {
        (ProviderConfigValueDto::List(items), ProviderConfigShape::List(value_type)) => {
            let items = clean_config_list(items);
            if let Some(bad) = items
                .iter()
                .find(|item| !provider_config_value_matches_type(item, value_type))
            {
                return invalid(format!(
                    "Invalid item \"{bad}\" for '{}': expected {value_type}",
                    key.name
                ));
            }
            Ok(Some(encode_config_list(&items)))
        }
        (ProviderConfigValueDto::Text(raw), ProviderConfigShape::List(value_type)) => {
            if !provider_config_key_accepts(key, raw) {
                return invalid(format!(
                    "Invalid value for '{}': expected a list of {value_type}",
                    key.name
                ));
            }
            Ok(Some(encode_config_list(&parse_config_list(raw))))
        }
        (ProviderConfigValueDto::List(_), ProviderConfigShape::Single(value_type)) => {
            invalid(format!(
                "Invalid value for '{}': expected {value_type}, not a list",
                key.name
            ))
        }
        (ProviderConfigValueDto::Text(raw), ProviderConfigShape::Single(value_type)) => {
            let candidate = raw.trim();
            if candidate.is_empty() {
                Ok(None)
            } else if provider_config_value_matches_type(candidate, value_type) {
                Ok(Some(candidate.to_string()))
            } else {
                invalid(format!(
                    "Invalid value for '{}': expected {value_type}",
                    key.name
                ))
            }
        }
    }
}

fn requirements_of(source: &Source) -> Result<Vec<ProviderConfigKey>, BackendError> {
    provider_registry()
        .into_iter()
        .find(|adapter| adapter.id() == *source)
        .map(|adapter| adapter.config_requirements().to_vec())
        .ok_or_else(|| BackendError::invalid_request(format!("Unknown provider '{source}'")))
}

pub(crate) fn build_snapshot(
    connections: &[Connection],
    current_os: &SupportedOs,
) -> ProviderConfigSnapshotDto {
    let registry = provider_registry();
    let kinds = registry
        .iter()
        .map(|adapter| {
            let source = adapter.id();
            ProviderKindDto {
                label: source.label().to_string(),
                description: adapter.description().to_string(),
                multiple: adapter.instances() == Instances::Multiple,
                supported_os: adapter.supported_os().to_vec(),
                supported: adapter.supported_os().contains(current_os),
                wsl_distros: wsl_distros(&source, current_os),
                source,
            }
        })
        .collect();
    let connections = connections
        .iter()
        .filter_map(|connection| {
            let adapter = registry
                .iter()
                .find(|adapter| adapter.id() == *connection.source())?;
            Some(ProviderConfigConnectionStateDto {
                connection: connection.id.clone(),
                label: connection.label.clone(),
                wsl_distro: match &connection.location {
                    ConnectionLocation::Wsl(distro) => Some(distro.clone()),
                    ConnectionLocation::Local => None,
                },
                fields: adapter
                    .config_requirements()
                    .iter()
                    .map(|key| ProviderConfigFieldStateDto {
                        key: key.name.to_string(),
                        label: provider_config_key_label(key.name),
                        required: key.required,
                        is_set: connection.values().get(key.name).is_some(),
                        value: field_value(connection, *key),
                    })
                    .collect(),
            })
        })
        .collect();
    ProviderConfigSnapshotDto { kinds, connections }
}

#[cfg_attr(not(target_os = "windows"), allow(unused_variables))]
fn wsl_distros(source: &Source, current_os: &SupportedOs) -> Vec<String> {
    #[cfg(target_os = "windows")]
    if *source == Source::Git && *current_os == SupportedOs::Windows {
        return activity_log_core::provider::git::windows::default_wsl_distros();
    }
    Vec::new()
}

/// The configured value with its type. An unset list shows the default the provider falls back
/// to, so the settings show what is actually used; a secret is never sent.
fn field_value(connection: &Connection, key: ProviderConfigKey) -> ProviderConfigFieldValueDto {
    let configured = connection
        .values()
        .get(key.name)
        .filter(|raw| !raw.trim().is_empty());
    match key.shape {
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
                None => default_list(connection, key),
            },
        },
    }
}

fn default_list(connection: &Connection, key: ProviderConfigKey) -> Vec<String> {
    if *connection.source() != Source::Git || key != GIT_DIRS {
        return Vec::new();
    }
    match &connection.location {
        ConnectionLocation::Wsl(_) => vec![WSL_HOME.to_string()],
        ConnectionLocation::Local => default_git_root()
            .map(|root| vec![root.display().to_string()])
            .unwrap_or_default(),
    }
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
        "CALENDAR" => "Calendar".to_string(),
        "ICS" => "ICS".to_string(),
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
