//! `activity-log sources …`: the connections in `config.toml`, managed without editing it.
//! Every answer is collected before the file is written, so no lock is held across a prompt.

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
    settings::{ConfigEditError, ConfigFile, field_name, key_for_field},
};
use anyhow::{Result, bail};

#[cfg(test)]
mod tests;

/// Why a `sources` command cannot go on; said in the terminal as it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SourcesError {
    /// Not `kind/name`, with the parser's reason.
    NotAnId(String),
    NoConnection(String),
    Ambiguous {
        text: String,
        first: ConnectionId,
        second: ConnectionId,
    },
    NoSettings(Source),
    UnknownSetting {
        source: Source,
        field: String,
    },
    TakesOneValue(ProviderConfigKey),
    InvalidValue(ProviderConfigKey),
}

impl std::fmt::Display for SourcesError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotAnId(reason) => write!(f, "{reason}"),
            Self::NoConnection(text) => {
                write!(f, "No connection {text}; see `activity-log sources`")
            }
            Self::Ambiguous {
                text,
                first,
                second,
            } => write!(f, "{text} is ambiguous: {first} or {second}"),
            Self::NoSettings(source) => write!(f, "{} has no settings", source.label()),
            Self::UnknownSetting { source, field } => write!(
                f,
                "{} has no setting {field}; use one of: {}",
                source.label(),
                fields_of(source)
            ),
            Self::TakesOneValue(key) => write!(f, "{} takes one value", key.name),
            Self::InvalidValue(key) => write!(f, "Invalid value: expected {}", key.shape),
        }
    }
}

impl std::error::Error for SourcesError {}

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
        Lines(std::io::stdin().lock()).ask(question, secret)
    }
}

/// Answers read line by line, such as piped input.
pub(crate) struct Lines<R>(pub R);

impl<R: BufRead> Prompt for Lines<R> {
    fn ask(&mut self, question: &str, _secret: bool) -> Result<String> {
        print!("{question}");
        std::io::stdout().flush()?;
        let mut line = String::new();
        // The input ended: no answer will come, so stop instead of asking again.
        if self.0.read_line(&mut line)? == 0 {
            bail!("The input ended before every question was answered; nothing was saved");
        }
        Ok(line.trim_end_matches(['\r', '\n']).to_string())
    }
}

/// `kind/name`, or just `name` when only one kind has a connection by that name.
pub(crate) fn resolve_connection(
    connections: &[Connection],
    text: &str,
) -> Result<ConnectionId, SourcesError> {
    let missing = || SourcesError::NoConnection(text.to_string());
    if text.contains('/') {
        let id = text
            .parse::<ConnectionId>()
            .map_err(SourcesError::NotAnId)?;
        return connections
            .iter()
            .any(|connection| connection.id == id)
            .then_some(id)
            .ok_or_else(missing);
    }
    let mut matches = connections
        .iter()
        .filter(|connection| connection.id.name == text)
        .map(|connection| connection.id.clone());
    match (matches.next(), matches.next()) {
        (Some(id), None) => Ok(id),
        (Some(first), Some(second)) => Err(SourcesError::Ambiguous {
            text: text.to_string(),
            first,
            second,
        }),
        (None, _) => Err(missing()),
    }
}

/// Asks for the values of a new connection of `source`; required ones until answered.
pub(crate) fn ask_values(
    source: &Source,
    prompt: &mut dyn Prompt,
) -> Result<Vec<(ProviderConfigKey, String)>> {
    let mut answers = Vec::new();
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
                    answers.push((key, raw));
                    break;
                }
                Err(err) => eprintln!("{err}"),
            }
        }
    }
    Ok(answers)
}

/// Adds a connection with the values asked for. Without a label it gets the kind's name.
pub(crate) fn add(
    file: &mut ConfigFile,
    source: &Source,
    label: Option<&str>,
    wsl: Option<String>,
    values: &[(ProviderConfigKey, String)],
) -> Result<ConnectionId, ConfigEditError> {
    let location = match wsl {
        Some(distro) => ConnectionLocation::Wsl(distro),
        None => ConnectionLocation::Local,
    };
    let id = file.add(source, label, location)?;
    for (key, raw) in values {
        file.set(&id, *key, Some(raw))?;
    }
    Ok(id)
}

/// The key `field` names for `id`, and the value to store: `values`, or the answer when there
/// are none (hidden for secrets).
pub(crate) fn value_for(
    id: &ConnectionId,
    field: &str,
    values: Vec<String>,
    prompt: &mut dyn Prompt,
) -> Result<(ProviderConfigKey, String)> {
    let key = key(&id.source, field)?;
    let values = if values.is_empty() {
        vec![prompt.ask(&format!("{field} for {id}: "), is_secret(key))?]
    } else {
        values
    };
    Ok((key, raw_value(key, &values)?))
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

pub(crate) fn key(source: &Source, field: &str) -> Result<ProviderConfigKey, SourcesError> {
    key_for_field(source, field).ok_or_else(|| {
        if keys_of(source).is_empty() {
            SourcesError::NoSettings(source.clone())
        } else {
            SourcesError::UnknownSetting {
                source: source.clone(),
                field: field.to_string(),
            }
        }
    })
}

fn is_secret(key: ProviderConfigKey) -> bool {
    key.value_type() == ProviderConfigValueType::Password
}

/// The stored form of `values` for `key`, checked against its type.
fn raw_value(key: ProviderConfigKey, values: &[String]) -> Result<String, SourcesError> {
    let raw = match key.shape {
        ProviderConfigShape::List(_) => encode_config_list(
            &values
                .iter()
                .flat_map(|value| parse_config_list(value))
                .collect::<Vec<_>>(),
        ),
        ProviderConfigShape::Single(_) => match values {
            [value] => value.trim().to_string(),
            _ => return Err(SourcesError::TakesOneValue(key)),
        },
    };
    if !provider_config_key_accepts(key, &raw) {
        return Err(SourcesError::InvalidValue(key));
    }
    Ok(raw)
}
