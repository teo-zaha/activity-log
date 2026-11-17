use std::{
    fs,
    path::{Path, PathBuf},
    process::Command as Process,
};

use activity_log_core::{
    collect_activity_detailed_with_provider_runtimes,
    model::{CollectOptions, CollectedActivity, OutputFormat, ProviderInfo, Source},
    provider::inspect_provider_runtimes_with,
    settings::{ConfigFile, load_connections},
    utils::to_markdown,
};
use anyhow::{Result, anyhow};
use chrono::{Days, Local};
use clap::{Args, Parser, Subcommand};
use strum::IntoEnumIterator;

mod range;
mod sources;

use range::{RangePreset, day_range, parse_date, resolve_preset, today};
use sources::{Terminal, list_lines, resolve_connection};

/// Collects your activity, or manages the sources it is collected from.
#[derive(Debug, Parser)]
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

#[derive(Debug, Args)]
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

#[derive(Debug, Subcommand)]
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

#[derive(Debug, Subcommand)]
enum SourcesCommand {
    /// Add a source, such as a GitLab account; asks for its values.
    Add {
        /// git, gitlab, jira, calendar, outlook or system.
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

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    /// Print the config file's path.
    Path,
    /// Open the config file in $VISUAL or $EDITOR.
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
            println!("{}", ConfigFile::resolve_path(config).display());
            Ok(())
        }
        Some(Command::Config {
            action: ConfigCommand::Edit,
        }) => edit_config(config),
    }
}

async fn list_sources(config: Option<&Path>) -> Result<()> {
    println!("{}\n", ConfigFile::resolve_path(config).display());
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

fn change_sources(action: SourcesCommand, config: Option<&Path>) -> Result<()> {
    let mut file = ConfigFile::load(ConfigFile::resolve_path(config))?;
    let connections = load_connections(config)?;
    let mut terminal = Terminal;
    match action {
        SourcesCommand::Add { kind, label, wsl } => {
            let id = sources::add(&mut file, &kind, label.as_deref(), wsl, &mut terminal)?;
            println!("Added {id}");
        }
        SourcesCommand::Set {
            source,
            field,
            values,
        } => {
            let id = resolve_connection(&connections, &source)?;
            sources::set(&mut file, &id, &field, values, &mut terminal)?;
        }
        SourcesCommand::Unset { source, field } => {
            let id = resolve_connection(&connections, &source)?;
            sources::unset(&mut file, &id, &field)?;
        }
        SourcesCommand::Rename { source, label } => {
            let id = resolve_connection(&connections, &source)?;
            file.rename(&id, &label)?;
        }
        SourcesCommand::Remove { source } => {
            let id = resolve_connection(&connections, &source)?;
            file.remove(&id)?;
            println!("Removed {id}");
        }
    }
    file.save()?;
    println!("Saved {}", file.path().display());
    Ok(())
}

fn edit_config(config: Option<&Path>) -> Result<()> {
    let path = ConfigFile::resolve_path(config);
    if !path.exists() {
        let file = ConfigFile::load(&path)?;
        file.save()?;
    }
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| {
            if cfg!(target_os = "windows") {
                "notepad".to_string()
            } else {
                "vi".to_string()
            }
        });
    let status = Process::new(&editor)
        .arg(&path)
        .status()
        .map_err(|err| anyhow!("Could not start {editor}: {err}"))?;
    if !status.success() {
        anyhow::bail!("{editor} exited with {status}");
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

    let content = match format {
        OutputFormat::Json => serde_json::to_string_pretty(&collected.log)?,
        OutputFormat::Markdown => to_markdown(&collected.log, &Local),
    };

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
