//! The settings pane's view of `config.toml`: every kind, each connection with its fields as
//! the pane shows them, and the one check and normalization of the values a writer submits
//! (the pane's saves, the CLI's `sources set`).

use crate::{
    model::{
        AddConnectionRequestDto, BackendError, ConnectionId, ProviderConfigConnectionStateDto,
        ProviderConfigFieldIssueDto, ProviderConfigFieldRefDto, ProviderConfigFieldStateDto,
        ProviderConfigFieldValueDto, ProviderConfigIssueKindDto, ProviderConfigSaveRequestDto,
        ProviderConfigSnapshotDto, ProviderConfigValueDto, ProviderKindDto, Source, SupportedOs,
    },
    provider::{
        connection::{Connection, ConnectionLocation, Instances},
        git::{GIT_DIRS, WSL_HOME, default_git_root},
        provider_config::{
            ConfigSource, ProviderConfigKey, ProviderConfigShape, ProviderConfigValueType,
            clean_config_list, encode_config_list, parse_config_list, url_with_credentials,
            value_problem,
        },
        provider_registry,
    },
    settings::{
        ConfigEditError, ConfigFile, Conflict, ConflictKind, EditBase, FieldId, Precondition,
        check_precondition, config_keys,
    },
};

#[cfg(test)]
mod tests;

/// A value as a writer submitted it.
#[derive(Clone, Copy)]
pub enum Submitted<'a> {
    /// A single value, or a list in its text form (a JSON array or comma-separated).
    Text(&'a str),
    /// A list as items; an empty list means "none", which differs from clearing.
    List(&'a [String]),
}

const ONE_VALUE: &str = "Enter one value, not a list.";
const NO_CREDENTIALS: &str = "Enter the address without a user name or password.";
const NO_SUCH_SETTING: &str = "This kind has no such setting.";
const NEEDS_LABEL: &str = "A connection needs a label.";
const CHANGED: &str = "Another program changed this since you started editing.";
const REMOVED: &str = "Another program removed this source.";
const REPLACED: &str = "Another program removed this source and added one of the same name.";
const UNKNOWN_BASE: &str = "The settings changed since they were read; read them again.";

/// The stored form of `value` for `key`, or what is wrong with it: `None` for a blank single
/// value, which clears the key. Items are trimmed and blank ones dropped; an issue about an
/// item names its place among the others.
pub fn normalize(
    key: ProviderConfigKey,
    value: Submitted<'_>,
) -> Result<Option<String>, Vec<ProviderConfigFieldIssueDto>> {
    let at_value = || ProviderConfigFieldRefDto::Value {
        key: key.name.to_string(),
    };
    match (key.shape, value) {
        (ProviderConfigShape::Single(_), Submitted::List(_)) => {
            Err(vec![issue(at_value(), ONE_VALUE)])
        }
        (ProviderConfigShape::Single(value_type), Submitted::Text(raw)) => {
            let raw = raw.trim();
            if raw.is_empty() {
                return Ok(None);
            }
            match new_value_problem(raw, value_type) {
                Some(message) => Err(vec![issue(at_value(), message)]),
                None => Ok(Some(raw.to_string())),
            }
        }
        (ProviderConfigShape::List(item_type), value) => {
            let items = match value {
                Submitted::List(items) => clean_config_list(items),
                Submitted::Text(raw) => parse_config_list(raw),
            };
            let issues = items
                .iter()
                .enumerate()
                .filter_map(|(index, item)| {
                    let message = new_value_problem(item, item_type)?;
                    let at = ProviderConfigFieldRefDto::Item {
                        key: key.name.to_string(),
                        index: u32::try_from(index).unwrap_or(u32::MAX),
                    };
                    Some(issue(at, message))
                })
                .collect::<Vec<_>>();
            if issues.is_empty() {
                Ok(Some(encode_config_list(&items)))
            } else {
                Err(issues)
            }
        }
    }
}

/// What a save or a removal was based on, as the writer's registry of the snapshots it handed
/// out knows the request's `basedOn`.
pub enum SaveBase {
    /// The request named no snapshot (an older dashboard): no precondition, as before.
    None,
    Known(EditBase),
    /// A snapshot no longer held, or one that did not have the connection.
    Unknown(ConnectionId),
}

impl SaveBase {
    /// The precondition the write checks under the lock; an unknown base is a conflict at once.
    pub fn precondition(self) -> Result<Precondition, Conflict> {
        match self {
            Self::None => Ok(Precondition::None),
            Self::Known(base) => Ok(Precondition::Unchanged(base)),
            Self::Unknown(connection) => Err(Conflict {
                connection,
                label: None,
                kind: ConflictKind::UnknownBase,
                fields: Vec::new(),
            }),
        }
    }
}

/// The fields a save changes, which its base must have seen as they are now.
pub fn touched_fields(request: &ProviderConfigSaveRequestDto) -> Vec<FieldId> {
    let keys = config_keys(&request.connection.source);
    let label = request.label.as_ref().map(|_| FieldId::Label);
    let values = request.values.iter().filter_map(|value| {
        keys.iter()
            .find(|key| key.name == value.key)
            .map(|key| FieldId::Value(key.name))
    });
    label.into_iter().chain(values).collect()
}

/// Every issue of a save, in request order: the label, then each value, items by index; then
/// what changed since its base, compared with `current`, the file as it is now (`None` without
/// a settings location). A check is advice: the file may change again before the save, whose
/// own precondition decides.
pub fn check_save(
    request: &ProviderConfigSaveRequestDto,
    base: SaveBase,
    current: Option<&ConfigFile>,
) -> Vec<ProviderConfigFieldIssueDto> {
    let mut issues = prepare_save(request).err().unwrap_or_default();
    let conflict = match (base.precondition(), current) {
        (Err(conflict), _) => Some(conflict),
        (Ok(precondition), Some(current)) => check_precondition(current, &precondition).err(),
        (Ok(_), None) => None,
    };
    issues.extend(conflict.iter().flat_map(conflict_issues));
    issues
}

/// A conflict as issues: one per changed field, or one for the connection as a whole.
pub fn conflict_issues(conflict: &Conflict) -> Vec<ProviderConfigFieldIssueDto> {
    let whole = |problem, message: &str| {
        vec![ProviderConfigFieldIssueDto {
            field: ProviderConfigFieldRefDto::Connection,
            problem,
            message: message.to_string(),
        }]
    };
    match conflict.kind {
        ConflictKind::Removed => whole(ProviderConfigIssueKindDto::Removed, REMOVED),
        ConflictKind::Replaced => whole(ProviderConfigIssueKindDto::Replaced, REPLACED),
        ConflictKind::UnknownBase => whole(ProviderConfigIssueKindDto::UnknownBase, UNKNOWN_BASE),
        ConflictKind::Changed => conflict
            .fields
            .iter()
            .map(|field| ProviderConfigFieldIssueDto {
                field: match field {
                    FieldId::Label => ProviderConfigFieldRefDto::Label,
                    FieldId::Value(key) => ProviderConfigFieldRefDto::Value {
                        key: (*key).to_string(),
                    },
                },
                problem: ProviderConfigIssueKindDto::Changed,
                message: CHANGED.to_string(),
            })
            .collect(),
    }
}

/// The pane's edits of one connection, checked against its kind's settings before the file is
/// read: the label, then each value in its stored form (`None` clears it).
pub struct SaveChanges {
    connection: ConnectionId,
    label: Option<String>,
    values: Vec<(ProviderConfigKey, Option<String>)>,
}

/// The edits of `request`, or `invalidRequest` with its first issue as `{field}: {message}`.
pub fn save_changes(request: &ProviderConfigSaveRequestDto) -> Result<SaveChanges, BackendError> {
    prepare_save(request).map_err(|issues| {
        let first = &issues[0];
        let keys = config_keys(&request.connection.source);
        BackendError::invalid_request(format!(
            "{}: {}",
            field_label(&first.field, keys),
            first.message
        ))
    })
}

/// One pass over `request`: its edits in their stored form, or every issue it has.
fn prepare_save(
    request: &ProviderConfigSaveRequestDto,
) -> Result<SaveChanges, Vec<ProviderConfigFieldIssueDto>> {
    let keys = config_keys(&request.connection.source);
    let mut issues = Vec::new();
    if request
        .label
        .as_deref()
        .is_some_and(|label| label.trim().is_empty())
    {
        issues.push(issue(ProviderConfigFieldRefDto::Label, NEEDS_LABEL));
    }
    let mut values = Vec::new();
    for value in &request.values {
        let Some(key) = keys.iter().copied().find(|key| key.name == value.key) else {
            let at = ProviderConfigFieldRefDto::Value {
                key: value.key.clone(),
            };
            issues.push(issue(at, NO_SUCH_SETTING));
            continue;
        };
        match value
            .value
            .as_ref()
            .map(|value| normalize(key, submitted(value)))
        {
            None => values.push((key, None)),
            Some(Ok(stored)) => values.push((key, stored)),
            Some(Err(found)) => issues.extend(found),
        }
    }
    if !issues.is_empty() {
        return Err(issues);
    }
    Ok(SaveChanges {
        connection: request.connection.clone(),
        label: request.label.clone(),
        values,
    })
}

/// Applies checked edits to the file as read under the config lock.
pub fn apply_save(file: &mut ConfigFile, changes: &SaveChanges) -> Result<(), ConfigEditError> {
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

pub fn apply_add(
    file: &mut ConfigFile,
    request: &AddConnectionRequestDto,
) -> Result<ConnectionId, ConfigEditError> {
    let location = match &request.wsl_distro {
        Some(distro) => ConnectionLocation::Wsl(distro.clone()),
        None => ConnectionLocation::Local,
    };
    file.add(&request.source, request.label.as_deref(), location)
}

/// Every kind, and `connections` with their fields as the settings show them, named by
/// `revision` (the writer's registry entry for the file they were read from).
pub fn build_snapshot(
    connections: &[Connection],
    current_os: &SupportedOs,
    revision: String,
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
                        label: key.label.to_string(),
                        required: key.required,
                        is_set: connection.values().get(key.name).is_some(),
                        value: field_value(connection, *key),
                    })
                    .collect(),
            })
        })
        .collect();
    ProviderConfigSnapshotDto {
        revision,
        kinds,
        connections,
    }
}

/// A new value's problem: a value the file may hold, and for a URL no user name or password.
fn new_value_problem(raw: &str, value_type: ProviderConfigValueType) -> Option<&'static str> {
    value_problem(raw, value_type).or_else(|| {
        (value_type == ProviderConfigValueType::Url && url_with_credentials(raw))
            .then_some(NO_CREDENTIALS)
    })
}

fn submitted(value: &ProviderConfigValueDto) -> Submitted<'_> {
    match value {
        ProviderConfigValueDto::Text(text) => Submitted::Text(text),
        ProviderConfigValueDto::List(items) => Submitted::List(items),
    }
}

/// A value that does not fit its setting.
fn issue(field: ProviderConfigFieldRefDto, message: &str) -> ProviderConfigFieldIssueDto {
    ProviderConfigFieldIssueDto {
        field,
        problem: ProviderConfigIssueKindDto::Invalid,
        message: message.to_string(),
    }
}

/// The field as the settings name it, for a refused save's message.
fn field_label(field: &ProviderConfigFieldRefDto, keys: &[ProviderConfigKey]) -> String {
    let key_label = |name: &str| {
        keys.iter()
            .find(|key| key.name == name)
            .map_or_else(|| name.to_string(), |key| key.label.to_string())
    };
    match field {
        ProviderConfigFieldRefDto::Label => "Label".to_string(),
        ProviderConfigFieldRefDto::Value { key } => key_label(key),
        ProviderConfigFieldRefDto::Item { key, index } => {
            format!("{} item {}", key_label(key), index + 1)
        }
        ProviderConfigFieldRefDto::Connection => "Source".to_string(),
    }
}

#[cfg_attr(not(target_os = "windows"), allow(unused_variables))]
fn wsl_distros(source: &Source, current_os: &SupportedOs) -> Vec<String> {
    #[cfg(target_os = "windows")]
    if *source == Source::Git && *current_os == SupportedOs::Windows {
        return crate::provider::git::windows::default_wsl_distros();
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
