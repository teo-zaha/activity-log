use std::{
    fs,
    path::{Path, PathBuf},
};

use activity_log_core::{
    collect_activity_detailed_with_provider_runtimes, diagnostics,
    export::render_export,
    model::{CollectOptions, CollectedActivity, ConnectionId, OutputFormat, ProviderInfo, Source},
    provider::{connection::ConnectionLocation, inspect_provider_runtimes_with},
    settings::{
        ConfigEditError, ConfigFile, EditError, ExternalEdit, FieldId, NO_LOCATION, Precondition,
        Editor, edit_config, edit_externally, load_connections,
    },
};
use anyhow::{Result, anyhow};
use chrono::{Days, Local};
use clap::{
    Args, Parser, Subcommand,
    builder::{PossibleValuesParser, TypedValueParser},
};
use strum::IntoEnumIterator;

mod range;
mod sources;

use range::{RangePreset, day_range, parse_date, resolve_preset, today};
use sources::{Terminal, list_lines, resolve_connection};

/// Collects your activity, or manages the sources it is collected from.
#[derive(Parser)]
#[command(author, version, about)]
struct Cli {
    /// The config file; default: ACTIVITY_LOG_CONFIG, else activity-log/config.toml in the OS
    /// config folder.
    #[arg(long, global = true, value_name = "PATH")]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<Command>,
    #[command(flatten)]
    collect: CollectArgs,
}

#[derive(Args)]
struct CollectArgs {
    /// First local day, YYYY-MM-DD.
    #[arg(long)]
    since: Option<String>,
    /// Last local day, YYYY-MM-DD.
    #[arg(long)]
    until: Option<String>,
    #[arg(long)]
    output: Option<String>,
    /// today, yesterday, week, lastweek, month or lastmonth.
    #[arg(long)]
    range: Option<String>,
    /// json or markdown.
    #[arg(long, default_value = "json")]
    format: String,
}

#[derive(Subcommand)]
enum Command {
    /// List the sources with their status, or add, change and remove them.
    Sources {
        #[command(subcommand)]
        action: Option<SourcesCommand>,
    },
    /// Show or edit the config file.
    Config {
        #[command(subcommand)]
        action: ConfigCommand,
    },
}

#[derive(Subcommand)]
enum SourcesCommand {
    /// Add a source, such as a GitLab account; asks for its values.
    Add {
        /// The kind of source.
        #[arg(value_parser = source_kinds())]
        kind: Source,
        /// Shown in lists and on entries; defaults to the kind's name.
        #[arg(long)]
        label: Option<String>,
        /// For git: a WSL distribution to read instead of this computer.
        #[arg(long, value_name = "DISTRO")]
        wsl: Option<String>,
    },
    /// Set a value, such as `sources set gitlab/work url https://…`; asks when no value is
    /// given, without echo for tokens.
    Set {
        /// kind/name, or just the name when it is unique.
        source: String,
        field: String,
        values: Vec<String>,
    },
    /// Remove a value, so its default applies again.
    Unset { source: String, field: String },
    /// Change the label shown for a source.
    Rename { source: String, label: String },
    /// Remove a source.
    Remove { source: String },
}

/// Every kind of source, by the name the config file and the commands use; `--help` lists them.
fn source_kinds() -> impl TypedValueParser<Value = Source> {
    PossibleValuesParser::new(Source::iter().map(<&'static str>::from))
        .try_map(|kind| kind.parse::<Source>())
}

#[derive(Subcommand)]
enum ConfigCommand {
    /// Print the config file's path.
    Path,
    /// Edit the config file in $VISUAL or $EDITOR. The editor gets a private copy, which is
    /// saved when it changed and the file did not change meanwhile.
    Edit,
}

#[derive(Debug, Clone)]
struct CliCollectionRequest {
    collect_options: CollectOptions,
    output_path: Option<String>,
}

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("Fatal: {err}");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    dotenvy::dotenv().ok();

    let cli = Cli::parse();
    let config = cli.config.as_deref();
    match cli.command {
        None => collect(&cli.collect, config).await,
        Some(Command::Sources { action: None }) => list_sources(config).await,
        Some(Command::Sources {
            action: Some(action),
        }) => change_sources(action, config),
        Some(Command::Config {
            action: ConfigCommand::Path,
        }) => {
            println!("{}", config_path(config)?.display());
            Ok(())
        }
        Some(Command::Config {
            action: ConfigCommand::Edit,
        }) => edit_in_editor(config),
    }
}

/// The settings file, or the message that there is no place for it.
fn config_path(config: Option<&Path>) -> Result<PathBuf> {
    ConfigFile::resolve_path(config).ok_or_else(|| anyhow!(NO_LOCATION))
}

async fn list_sources(config: Option<&Path>) -> Result<()> {
    if let Some(path) = ConfigFile::resolve_path(config) {
        println!("{}\n", path.display());
    }
    let runtimes = inspect_provider_runtimes_with(&load_connections(config)?).await;
    let providers = runtimes
        .iter()
        .map(|runtime| runtime.info().clone())
        .collect::<Vec<_>>();
    for line in list_lines(&providers) {
        println!("{line}");
    }
    Ok(())
}

/// A change of the settings, applied to the file as read under the lock.
type Change = Box<dyn FnOnce(&mut ConfigFile) -> Result<Option<String>, ConfigEditError>>;

/// Asks what it needs first, then edits the file once. A command that showed the user a value
/// (a question) saves only if the connection is still as it was then.
fn change_sources(action: SourcesCommand, config: Option<&Path>) -> Result<()> {
    let path = config_path(config)?;
    let file = ConfigFile::load(&path)?;
    let connections = file.logged_connections();
    let mut terminal = Terminal;
    let (precondition, change): (Precondition, Change) = match action {
        SourcesCommand::Add { kind, label, wsl } => {
            // Checked before the questions, so a refused add asks nothing.
            let location = wsl
                .clone()
                .map_or(ConnectionLocation::Local, ConnectionLocation::Wsl);
            file.clone().add(&kind, label.as_deref(), location)?;
            let values = sources::ask_values(&kind, &mut terminal)?;
            let change: Change = Box::new(move |file| {
                let id = sources::add(file, &kind, label.as_deref(), wsl, &values)?;
                Ok(Some(format!("Added {id}")))
            });
            (Precondition::None, change)
        }
        SourcesCommand::Set {
            source,
            field,
            values,
        } => {
            let id = resolve_connection(&connections, &source)?;
            let asks = values.is_empty();
            let (key, raw) = sources::value_for(&id, &field, values, &mut terminal)?;
            let precondition = match file.edit_base(&id, &[FieldId::Value(key.name)]) {
                Some(base) if asks => Precondition::Unchanged(base),
                _ => Precondition::None,
            };
            let change: Change = Box::new(move |file| {
                file.set(&id, key, raw.as_deref())?;
                Ok(None)
            });
            (precondition, change)
        }
        SourcesCommand::Unset { source, field } => {
            let id = resolve_connection(&connections, &source)?;
            let key = sources::key(&id.source, &field)?;
            let change: Change = Box::new(move |file| {
                file.set(&id, key, None)?;
                Ok(None)
            });
            (Precondition::None, change)
        }
        SourcesCommand::Rename { source, label } => {
            let id = resolve_connection(&connections, &source)?;
            let change: Change = Box::new(move |file| {
                file.rename(&id, &label)?;
                Ok(None)
            });
            (Precondition::None, change)
        }
        SourcesCommand::Remove { source } => {
            let id = resolve_connection(&connections, &source)?;
            let change: Change = Box::new(move |file| {
                file.remove(&id)?;
                Ok(Some(format!("Removed {id}")))
            });
            (Precondition::None, change)
        }
    };
    let label = |id: &ConnectionId| {
        connections
            .iter()
            .find(|connection| connection.id == *id)
            .map(|connection| connection.label.clone())
    };
    let edited =
        edit_config(&path, precondition, change).map_err(|err| edit_failure(err, label))?;
    if let Some(message) = edited.value {
        println!("{message}");
    }
    for warning in edited.warnings {
        diagnostics::warn("config", warning);
    }
    println!("Saved {}", path.display());
    Ok(())
}

/// An edit's failure as the terminal says it, a conflicting connection named by `label`; the
/// typed `EditError` stays below the message as its cause.
fn edit_failure(err: EditError, label: impl Fn(&ConnectionId) -> Option<String>) -> anyhow::Error {
    let label = match &err {
        EditError::Conflict(conflict) => label(&conflict.connection),
        _ => None,
    };
    let message = err.message(label.as_deref());
    anyhow::Error::new(err).context(message)
}

fn edit_in_editor(config: Option<&Path>) -> Result<()> {
    let path = config_path(config)?;
    let editor = Editor::from_env();
    let outcome = edit_externally(&path, |copy| {
        let status = editor
            .command()
            .arg(copy)
            .status()
            .map_err(|err| anyhow!("Could not start {}: {err}", editor.name()))?;
        if !status.success() {
            anyhow::bail!("{} exited with {status}", editor.name());
        }
        Ok(())
    })?;
    match outcome {
        ExternalEdit::Unchanged => println!("No changes"),
        ExternalEdit::Saved { warnings } => {
            for warning in warnings {
                diagnostics::warn("config", warning);
            }
            println!("Saved {}", path.display());
        }
    }
    Ok(())
}

async fn collect(args: &CollectArgs, config: Option<&Path>) -> Result<()> {
    let format = parse_format(&args.format)?;
    let request = build_collect_request(args, &format)?;
    let connections = load_connections(config)?;
    if connections.is_empty() {
        println!("{}", sources::NO_SOURCES);
        return Ok(());
    }

    println!(
        "\nCollecting activity from {} to {} ...\n",
        request.collect_options.start.with_timezone(&Local),
        request.collect_options.end.with_timezone(&Local)
    );

    let provider_runtimes = inspect_provider_runtimes_with(&connections).await;
    let provider_info = provider_runtimes
        .iter()
        .map(|runtime| runtime.info().clone())
        .collect::<Vec<_>>();
    let collected = collect_activity_detailed_with_provider_runtimes(
        request.collect_options,
        &provider_runtimes,
    )
    .await;

    for line in provider_summary_lines(&provider_info, &collected) {
        println!("{line}");
    }

    let content = render_export(&collected.log, format, &Local).content;

    if let Some(output) = &request.output_path {
        if let Some(parent) = Path::new(output).parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(output, content)?;
        println!(
            "\n✅ {} total entries → {}\n",
            collected.log.entries.len(),
            output
        );
    } else {
        println!("{content}");
    }

    Ok(())
}

fn parse_format(value: &str) -> Result<OutputFormat> {
    value
        .parse::<OutputFormat>()
        .map_err(|_| anyhow!("Invalid --format \"{value}\". Use json or markdown."))
}

fn build_collect_request(cli: &CollectArgs, format: &OutputFormat) -> Result<CliCollectionRequest> {
    let default_output = match format {
        OutputFormat::Json => "logs/activity_log.json",
        OutputFormat::Markdown => "logs/activity_log.md",
    };

    let (since, until) = if let Some(range) = &cli.range {
        let preset = range
            .parse::<RangePreset>()
            .map_err(|_| anyhow!("Unknown range: \"{range}\". Use {}", valid_range_presets()))?;
        resolve_preset(preset, today())
    } else {
        let since = match &cli.since {
            Some(since) => parse_date(since, "since")?,
            None => today() - Days::new(30),
        };
        let until = match &cli.until {
            Some(until) => parse_date(until, "until")?,
            None => today(),
        };
        (since, until)
    };

    Ok(CliCollectionRequest {
        collect_options: day_range(&Local, since, until)?,
        output_path: Some(
            cli.output
                .clone()
                .unwrap_or_else(|| default_output.to_string()),
        ),
    })
}

fn valid_range_presets() -> String {
    RangePreset::iter()
        .map(|preset| preset.to_string())
        .collect::<Vec<_>>()
        .join("|")
}

fn provider_summary_lines(
    provider_info: &[ProviderInfo],
    collected: &CollectedActivity,
) -> Vec<String> {
    provider_info
        .iter()
        .map(|info| {
            let id = &info.connection;
            if !info.usable {
                let reason = info.reason.as_deref().unwrap_or("unavailable");
                return format!("  - {id} ({}) — skipped ({reason})", info.label);
            }

            let count = collected.summary.entry_count_for(id);
            let detail = match id.source {
                Source::Git => format!(
                    " ({count} entries, {} merged with gitlab)",
                    collected.summary.merged_count
                ),
                Source::Gitlab => format!(
                    " ({count} entries, {} push events merged into git)",
                    collected.summary.merged_count
                ),
                _ => format!(" ({count} entries)"),
            };

            format!("  ✓ {id} ({}){detail}", info.label)
        })
        .collect()
}

#[cfg(test)]
mod tests;
