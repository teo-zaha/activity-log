//! `activity-log sources …`: the connections in `config.toml`, managed without editing it.

use std::io::{BufRead, IsTerminal, Write};

use activity_log_core::{
    model::{ConnectionId, ProviderInfo, Source},
    provider::{
        connection::{Connection, ConnectionLocation},
        provider_config::{
            ProviderConfigKey, ProviderConfigShape, ProviderConfigValueType, encode_config_list,
            parse_config_list, provider_config_key_accepts,
        },
        provider_registry,
    },
    settings::{ConfigFile, field_name, key_for_field},
};
use anyhow::{Result, anyhow, bail};

#[cfg(test)]
mod tests;

pub(crate) const NO_SOURCES: &str =
    "No sources yet. Add one with `activity-log sources add <kind>`, such as `sources add git`.";

/// Asks the user for a value; secrets are read without echo.
pub(crate) trait Prompt {
    fn ask(&mut self, question: &str, secret: bool) -> Result<String>;
}

pub(crate) struct Terminal;

impl Prompt for Terminal {
    fn ask(&mut self, question: &str, secret: bool) -> Result<String> {
        // Without a terminal (input piped in a script) secrets are read like any other line.
        if secret && std::io::stdin().is_terminal() {
            return Ok(rpassword::prompt_password(question)?);
        }
        print!("{question}");
        std::io::stdout().flush()?;
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line)?;
        Ok(line.trim_end_matches(['\r', '\n']).to_string())
    }
}

/// `kind/name`, or just `name` when only one kind has a connection by that name.
pub(crate) fn resolve_connection(connections: &[Connection], text: &str) -> Result<ConnectionId> {
    if text.contains('/') {
        let id = text.parse::<ConnectionId>().map_err(|err| anyhow!(err))?;
        return connections
            .iter()
            .any(|connection| connection.id == id)
            .then_some(id)
            .ok_or_else(|| anyhow!("No connection {text}; see `activity-log sources`"));
    }
    let mut matches = connections
        .iter()
        .filter(|connection| connection.id.name == text)
        .map(|connection| connection.id.clone());
    match (matches.next(), matches.next()) {
        (Some(id), None) => Ok(id),
        (Some(first), Some(second)) => bail!("{text} is ambiguous: {first} or {second}"),
        (None, _) => bail!("No connection {text}; see `activity-log sources`"),
    }
}

/// Adds a connection and asks for its values. Without a label it gets the kind's name.
pub(crate) fn add(
    file: &mut ConfigFile,
    source: &Source,
    label: Option<&str>,
    wsl: Option<String>,
    prompt: &mut dyn Prompt,
) -> Result<ConnectionId> {
    let location = match wsl {
        Some(distro) => ConnectionLocation::Wsl(distro),
        None => ConnectionLocation::Local,
    };
    let id = file.add(source, label, location)?;
    for key in keys_of(source) {
        let field = field_name(source, key);
        let hint = match (key.required, key.shape) {
            (true, _) => format!("{field} ({}, required): ", key.shape),
            (false, ProviderConfigShape::List(_)) => {
                format!("{field} ({}, comma-separated, Enter to skip): ", key.shape)
            }
            (false, _) => format!("{field} ({}, Enter to skip): ", key.shape),
        };
        loop {
            let answer = prompt.ask(&hint, is_secret(key))?;
            if answer.trim().is_empty() {
                if key.required {
                    eprintln!("{field} is required");
                    continue;
                }
                break;
            }
            match raw_value(key, &[answer]) {
                Ok(raw) => {
                    file.set(&id, key, Some(&raw))?;
                    break;
                }
                Err(err) => eprintln!("{err}"),
            }
        }
    }
    Ok(id)
}

/// Sets one field; without values, asks for it (hidden for secrets).
pub(crate) fn set(
    file: &mut ConfigFile,
    id: &ConnectionId,
    field: &str,
    values: Vec<String>,
    prompt: &mut dyn Prompt,
) -> Result<()> {
    let key = key(&id.source, field)?;
    let values = if values.is_empty() {
        vec![prompt.ask(&format!("{field} for {id}: "), is_secret(key))?]
    } else {
        values
    };
    let raw = raw_value(key, &values)?;
    file.set(id, key, Some(&raw))
}

/// Removes a field, so its default applies again.
pub(crate) fn unset(file: &mut ConfigFile, id: &ConnectionId, field: &str) -> Result<()> {
    let key = key(&id.source, field)?;
    file.set(id, key, None)
}

/// One line per connection: id, label and status.
pub(crate) fn list_lines(providers: &[ProviderInfo]) -> Vec<String> {
    if providers.is_empty() {
        return vec![NO_SOURCES.to_string()];
    }
    let width = providers
        .iter()
        .map(|info| info.connection.to_string().len())
        .max()
        .unwrap_or(0);
    let label_width = providers
        .iter()
        .map(|info| info.label.chars().count())
        .max()
        .unwrap_or(0);
    providers
        .iter()
        .map(|info| {
            let status = match (&info.reason, info.usable) {
                (None, true) => "✓ ready".to_string(),
                (Some(reason), true) => format!("! {reason}"),
                (reason, false) => format!("✗ {}", reason.as_deref().unwrap_or("unavailable")),
            };
            format!(
                "{:width$}  {:label_width$}  {status}",
                info.connection.to_string(),
                info.label,
            )
        })
        .collect()
}

/// The fields a kind accepts, for messages: `url, token, user`.
pub(crate) fn fields_of(source: &Source) -> String {
    keys_of(source)
        .into_iter()
        .map(|key| field_name(source, key))
        .collect::<Vec<_>>()
        .join(", ")
}

fn keys_of(source: &Source) -> Vec<ProviderConfigKey> {
    provider_registry()
        .into_iter()
        .find(|adapter| adapter.id() == *source)
        .map(|adapter| adapter.config_requirements().to_vec())
        .unwrap_or_default()
}

fn key(source: &Source, field: &str) -> Result<ProviderConfigKey> {
    key_for_field(source, field).ok_or_else(|| {
        let fields = fields_of(source);
        if fields.is_empty() {
            anyhow!("{} has no settings", source.label())
        } else {
            anyhow!(
                "{} has no setting {field}; use one of: {fields}",
                source.label()
            )
        }
    })
}

fn is_secret(key: ProviderConfigKey) -> bool {
    key.value_type() == ProviderConfigValueType::Password
}

/// The stored form of `values` for `key`, checked against its type.
fn raw_value(key: ProviderConfigKey, values: &[String]) -> Result<String> {
    let raw = match key.shape {
        ProviderConfigShape::List(_) => encode_config_list(
            &values
                .iter()
                .flat_map(|value| parse_config_list(value))
                .collect::<Vec<_>>(),
        ),
        ProviderConfigShape::Single(_) => match values {
            [value] => value.trim().to_string(),
            _ => bail!("{} takes one value", key.name),
        },
    };
    if !provider_config_key_accepts(key, &raw) {
        bail!("Invalid value: expected {}", key.shape);
    }
    Ok(raw)
}
