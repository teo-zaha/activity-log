use std::{
    ffi::OsString,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::ExitCode,
};

use activity_log_core::{
    collect_runtimes, diagnostics,
    export::render_export,
    model::{CollectOptions, ConnectionId, LegacyImportReportDto, OutputFormat, Source},
    provider::{
        connection::ConnectionLocation, current_os, inspect_provider_runtimes_with,
        select_fix_action,
    },
    runtime::FixSurface,
    secrets::{FrontEnd, Interaction, Secrets},
    settings::{
        ConfigEditError, ConfigFile, EditError, Editor, ExternalEdit, FieldId, NO_LOCATION,
        Precondition, edit_config, edit_externally, legacy_import, load_connections,
    },
};
use anyhow::{Result, anyhow};
use chrono::{Days, Local, NaiveDate};
use clap::{
    Args, Parser, Subcommand,
    builder::{OsStringValueParser, PossibleValuesParser, TypedValueParser},
};
use strum::IntoEnumIterator;

mod range;
mod results;
mod sources;

use range::{RangePreset, day_range, parse_day, resolve_preset, today};
use results::connection_results;
use sources::{SourcesError, Terminal, list_lines, resolve_connection};

/// What the exit status says; the README says it in the same words (a test checks).
const EXIT_STATUS: &str = "Exit status: 0 when every source was collected or skipped (another OS, not installed, not running, nothing to read); 1 when a source failed (refused, server busy, unreachable, timed out, not configured, not permitted, its check or its collection failed), after the export of the others is written, or on any error, an invalid flag or value included.";

/// Collects your activity, or manages the sources it is collected from.
#[derive(Parser)]
#[command(
    author,
    version,
    about,
    after_help = EXIT_STATUS
)]
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
    /// First local day.
    #[arg(long, value_name = "YYYY-MM-DD", value_parser = parse_day)]
    since: Option<NaiveDate>,
    /// Last local day.
    #[arg(long, value_name = "YYYY-MM-DD", value_parser = parse_day)]
    until: Option<NaiveDate>,
    /// Where the export goes, `-` for standard output; default: logs/activity_log.json or .md.
    #[arg(long, value_name = "PATH", value_parser = output_target())]
    output: Option<Output>,
    /// A range of days instead of --since and --until; weeks start on Monday.
    #[arg(long, value_parser = spellings::<RangePreset>(), ignore_case = true)]
    range: Option<RangePreset>,
    #[arg(long, default_value = "json", value_parser = spellings::<OutputFormat>(), ignore_case = true)]
    format: OutputFormat,
}

/// Where the export goes.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Output {
    File(PathBuf),
    /// `-`: the export alone on standard output, everything else on standard error.
    Stdout,
}

impl Output {
    fn default_for(format: OutputFormat) -> Self {
        Self::File(PathBuf::from(format!(
            "logs/activity_log.{}",
            format.extension()
        )))
    }
}

fn output_target() -> impl TypedValueParser<Value = Output> {
    OsStringValueParser::new().map(|value: OsString| match value.to_str() {
        Some("-") => Output::Stdout,
        _ => Output::File(PathBuf::from(value)),
    })
}

/// The values of an enum, by the names it parses, so `--help` lists them and every spelling
/// comes from the enum.
fn spellings<T>() -> impl TypedValueParser<Value = T>
where
    T: IntoEnumIterator + Into<&'static str> + std::str::FromStr + Clone + Send + Sync + 'static,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    PossibleValuesParser::new(T::iter().map(Into::<&'static str>::into))
        // The names are lowercase; `ignore_case` lets any case through to here.
        .try_map(|value| value.to_ascii_lowercase().parse::<T>())
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
        #[arg(value_parser = spellings::<Source>(), ignore_case = true)]
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
        /// The source's secret (a token) saved in the same edit, `-` to read it from standard
        /// input: a new address needs a sealed token entered again.
        #[arg(long, value_name = "VALUE|-")]
        secret: Option<String>,
    },
    /// Remove a value, so its default applies again.
    Unset { source: String, field: String },
    /// Change the label shown for a source.
    Rename { source: String, label: String },
    /// Remove a source.
    Remove { source: String },
}

#[derive(Subcommand)]
enum ConfigCommand {
    /// Print the config file's path.
    Path,
    /// Edit the config file in $VISUAL or $EDITOR. The editor gets a private copy, which is
    /// saved when it changed and the file did not change meanwhile.
    Edit,
    /// Give up importing Activity Log 0.0.7's settings, which waits (for the keychain, or a file
    /// to be fixed): commands that write the settings then work again.
    SkipImport,
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(err) => {
            // Help and version go to stdout and succeed; any other refusal is an error, which
            // exits 1 like every other error.
            let _ = err.print();
            return if err.use_stderr() {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            };
        }
    };
    // First, so every path below is the profile's.
    if let Err(err) = activity_log_core::profile::init(cli.config.as_deref()) {
        return fatal(&err.into());
    }
    match run(cli).await {
        Ok(status) => status,
        Err(err) => fatal(&err),
    }
}

/// What a command does with 0.0.7's settings before it runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Import {
    /// It reads or writes no source (`config path`, `config skip-import`).
    None,
    /// It reads the sources: the import runs first, and one that waits leaves it be.
    Reads,
    /// It writes `config.toml`, which would give up an import that waits: it is refused then.
    Writes,
}

impl Import {
    fn of(command: Option<&Command>) -> Self {
        match command {
            Some(Command::Config {
                action: ConfigCommand::Path | ConfigCommand::SkipImport,
            }) => Self::None,
            Some(Command::Config {
                action: ConfigCommand::Edit,
            })
            | Some(Command::Sources { action: Some(_) }) => Self::Writes,
            None | Some(Command::Sources { action: None }) => Self::Reads,
        }
    }
}

/// A command that writes the settings while 0.0.7's import is due and not done: the import's
/// own lines said why.
#[derive(Debug)]
struct ImportWaits;

impl std::fmt::Display for ImportWaits {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "Activity Log 0.0.7's sources are not imported yet, and this command would write config.toml, which gives the import up. Run any `activity-log` command again once the keychain can answer (or the file is fixed) to import them, or give the import up with `activity-log config skip-import`.",
        )
    }
}

impl std::error::Error for ImportWaits {}

/// 0.0.7's sources, imported once on the default config path. What it says goes to standard
/// error: its warnings as they are logged, and the line naming what it imported. A command that
/// writes the settings is refused while the import waits ([`ImportWaits`]).
async fn import_legacy(config: Option<&Path>, import: Import) -> Result<()> {
    if import == Import::None {
        return Ok(());
    }
    let Some(place) = legacy_import::Place::for_front_end(FrontEnd::Cli, config) else {
        return Ok(());
    };
    let secrets = Secrets::cli(ConfigFile::resolve_path(config).as_deref());
    match legacy_import::import_once(&place, &secrets, Interaction::Allowed).await {
        Some(LegacyImportReportDto::Imported { message, .. }) => {
            let _ = writeln!(io::stderr(), "{message}");
            Ok(())
        }
        Some(
            LegacyImportReportDto::Postponed { .. }
            | LegacyImportReportDto::Unreadable { .. }
            | LegacyImportReportDto::Failed { .. },
        ) if import == Import::Writes => Err(ImportWaits.into()),
        Some(_) | None => Ok(()),
    }
}

/// `config skip-import`: the marker that gives 0.0.7's import up, where it is due.
fn skip_import(config: Option<&Path>) -> Result<()> {
    let mut status = Status::stdout();
    let given_up = match legacy_import::Place::for_front_end(FrontEnd::Cli, config) {
        Some(place) => legacy_import::give_up(&place)?,
        None => legacy_import::GiveUp::NotDue,
    };
    match given_up {
        legacy_import::GiveUp::GaveUp => {
            status.say("The import of Activity Log 0.0.7's settings was given up")?
        }
        legacy_import::GiveUp::NotDue => status.say("No import of 0.0.7's settings is due")?,
    }
    Ok(())
}

fn fatal(err: &anyhow::Error) -> ExitCode {
    // With standard error gone there is nowhere left to say it.
    let _ = writeln!(io::stderr(), "Fatal: {err}");
    ExitCode::FAILURE
}

async fn run(cli: Cli) -> Result<ExitCode> {
    let config = cli.config.as_deref();
    // Every command that reads the sources first imports 0.0.7's, where that is due.
    import_legacy(config, Import::of(cli.command.as_ref())).await?;
    match cli.command {
        None => return collect(&cli.collect, config).await,
        Some(Command::Sources { action: None }) => list_sources(config).await?,
        Some(Command::Sources {
            action: Some(action),
        }) => change_sources(action, config).await?,
        Some(Command::Config {
            action: ConfigCommand::Path,
        }) => Status::stdout().say(&config_path(config)?.display().to_string())?,
        Some(Command::Config {
            action: ConfigCommand::Edit,
        }) => edit_in_editor(config)?,
        Some(Command::Config {
            action: ConfigCommand::SkipImport,
        }) => skip_import(config)?,
    }
    Ok(ExitCode::SUCCESS)
}

/// The settings file, or the message that there is no place for it.
fn config_path(config: Option<&Path>) -> Result<PathBuf> {
    ConfigFile::resolve_path(config).ok_or_else(|| anyhow!(NO_LOCATION))
}

async fn list_sources(config: Option<&Path>) -> Result<()> {
    let mut status = Status::stdout();
    if let Some(path) = ConfigFile::resolve_path(config) {
        status.say(&format!("{}\n", path.display()))?;
    }
    let secrets = Secrets::cli(ConfigFile::resolve_path(config).as_deref());
    let connections = load_connections(config)?;
    let runtimes = inspect_provider_runtimes_with(&connections, &secrets).await;
    let os = current_os();
    let providers = runtimes
        .iter()
        .zip(&connections)
        .map(|(runtime, connection)| {
            let mut info = runtime.info().clone();
            info.fix_action = select_fix_action(FixSurface::Web, &os, connection, &mut info, None);
            info
        })
        .collect::<Vec<_>>();
    for line in list_lines(&providers) {
        status.say(&line)?;
    }
    Ok(())
}

/// A change of the settings, applied to the file as read under the lock.
type Change = Box<dyn FnOnce(&mut ConfigFile) -> Result<Option<String>, ConfigEditError>>;

/// Asks what it needs first, then edits the file once. A command that showed the user a value
/// (a question) saves only if the connection is still as it was then.
async fn change_sources(action: SourcesCommand, config: Option<&Path>) -> Result<()> {
    let path = config_path(config)?;
    let file = ConfigFile::load(&path)?;
    let connections = file.logged_connections();
    let mut terminal = Terminal;
    let mut brings_secret = false;
    let (precondition, change): (Precondition, Change) = match action {
        SourcesCommand::Add { kind, label, wsl } => {
            // Checked before the questions, so a refused add asks nothing.
            let location = wsl
                .clone()
                .map_or(ConnectionLocation::Local, ConnectionLocation::Wsl);
            file.clone().add(&kind, label.as_deref(), location)?;
            let values = sources::ask_values(&kind, &mut terminal)?;
            brings_secret = values.iter().any(|(key, _)| key.is_secret());
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
            secret,
        } => {
            let id = resolve_connection(&connections, &source)?;
            let asks = values.is_empty();
            let mut set = vec![sources::value_for(&id, &field, values, &mut terminal)?];
            if let Some(secret) = secret {
                let key = sources::secret_of(&id.source)?;
                if set[0].0 == key {
                    return Err(SourcesError::SecretTwice(key).into());
                }
                let secret_field = sources::field_of(&id.source, key);
                set.push(sources::value_for(
                    &id,
                    &secret_field,
                    vec![secret],
                    &mut terminal,
                )?);
            }
            // A new address (or TLS setting) needs the token sealed for it: asked here.
            let mut moved = file.clone();
            for (key, raw) in &set {
                moved.set(&id, *key, raw.as_deref())?;
            }
            if let Some((_, secret)) = moved.rebound(&file).into_iter().next() {
                if !std::io::IsTerminal::is_terminal(&io::stdin()) {
                    return Err(SourcesError::SecretNeeded { id, secret, field }.into());
                }
                let field = sources::field_of(&id.source, secret);
                set.push(sources::value_for(&id, &field, Vec::new(), &mut terminal)?);
            }
            brings_secret = set
                .iter()
                .any(|(key, raw)| key.is_secret() && raw.is_some());
            let touched = set
                .iter()
                .map(|(key, _)| FieldId::Value(key.name))
                .collect::<Vec<_>>();
            let precondition = match file.edit_base(&id, &touched) {
                Some(base) if asks => Precondition::Unchanged(base),
                _ => Precondition::None,
            };
            let change: Change = Box::new(move |file| {
                for (key, raw) in &set {
                    file.set(&id, *key, raw.as_deref())?;
                }
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
    // The key is read before the file is locked: the keychain may ask, for as long as it takes.
    let sealing = match brings_secret {
        true => Some(
            Secrets::cli(Some(&path))
                .sealer(Interaction::Allowed, |store| file.sealed_in(store))
                .await
                .map_err(|err| edit_failure(EditError::SecretStore(err), label))?,
        ),
        false => None,
    };
    let edited = edit_config(&path, precondition, sealing.as_ref(), change)
        .map_err(|err| edit_failure(err, label))?;
    let mut status = Status::stdout();
    if let Some(message) = edited.value {
        status.say(&message)?;
    }
    for warning in edited.warnings {
        diagnostics::warn("config", warning);
    }
    status.say(&format!("Saved {}", path.display()))?;
    Ok(())
}

/// An edit's failure as the terminal says it, a conflicting connection named by `label`; the
/// typed `EditError` stays below the message as its cause.
fn edit_failure(err: EditError, label: impl Fn(&ConnectionId) -> Option<String>) -> anyhow::Error {
    let label = match &err {
        EditError::Conflict(conflict) => label(&conflict.connection),
        _ => None,
    };
    let mut message = err.message(label.as_deref());
    if let EditError::SecretStore(store) = &err
        && store.kind() != activity_log_core::secrets::StoreErrorKind::Unavailable
    {
        message.push_str(
            "\nRun it in the desktop session, where the keychain can ask, or set ACTIVITY_LOG_SECRET_STORE=file to keep the key in a file.",
        );
    }
    anyhow::Error::new(err).context(message)
}

fn edit_in_editor(config: Option<&Path>) -> Result<()> {
    let path = config_path(config)?;
    let editor = Editor::from_env();
    let outcome = edit_externally(
        &path,
        |copy| {
            let status = editor
                .command()
                .arg(copy)
                .status()
                .map_err(|err| anyhow!("Could not start {}: {err}", editor.name()))?;
            if !status.success() {
                anyhow::bail!("{} exited with {status}", editor.name());
            }
            Ok(())
        },
        |edited| {
            // Only for a new plain secret, after the editor closed and before the file is locked.
            let secrets = Secrets::cli(Some(&path));
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current()
                    .block_on(secrets.sealer(Interaction::Allowed, |store| edited.sealed_in(store)))
            })
        },
    )?;
    let mut status = Status::stdout();
    match outcome {
        ExternalEdit::Unchanged => status.say("No changes")?,
        ExternalEdit::Saved { warnings } => {
            for warning in warnings {
                diagnostics::warn("config", warning);
            }
            status.say(&format!("Saved {}", path.display()))?;
        }
    }
    Ok(())
}

/// Collects the range `args` name from every source, writes the export, and says how each
/// source did. A source that failed fails the run, after the others' entries are written.
async fn collect(args: &CollectArgs, config: Option<&Path>) -> Result<ExitCode> {
    let output = args
        .output
        .clone()
        .unwrap_or_else(|| Output::default_for(args.format));
    let mut status = Status::for_output(&output);
    let options = collect_options(args, today())?;
    let connections = load_connections(config)?;
    if connections.is_empty() {
        status.say(sources::NO_SOURCES)?;
        return Ok(ExitCode::SUCCESS);
    }

    status.say(&format!(
        "\nCollecting activity from {} to {} ...\n",
        options.start.with_timezone(&Local),
        options.end.with_timezone(&Local)
    ))?;

    let secrets = Secrets::cli(ConfigFile::resolve_path(config).as_deref());
    let runtimes = inspect_provider_runtimes_with(&connections, &secrets).await;
    let providers = runtimes
        .iter()
        .map(|runtime| runtime.info().clone())
        .collect::<Vec<_>>();
    let report = collect_runtimes(options, &runtimes).await;
    let collected = report.project(options);
    let results = connection_results(&providers, &report, &collected.summary);
    for result in &results {
        status.say(&result.line())?;
    }
    if collected.summary.merged_count > 0 {
        status.say(&format!(
            "  Git commits with code host activity ({})",
            collected.summary.merged_count
        ))?;
    }

    let content = render_export(&collected.log, args.format, &Local).content;
    match &output {
        Output::File(path) => {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(path, content)?;
            status.say(&format!(
                "\n✅ {} total entries → {}\n",
                collected.log.entries.len(),
                path.display()
            ))?;
        }
        Output::Stdout => write_stdout(content.as_bytes())?,
    }

    let failed = results.iter().filter(|result| result.fails_run()).count();
    if failed == 0 {
        return Ok(ExitCode::SUCCESS);
    }
    status.say(&format!("{failed} of {} sources failed", results.len()))?;
    Ok(ExitCode::FAILURE)
}

/// The instants of the days `args` name, `today` in the terminal's zone: a range preset, else
/// `--since` (30 days ago) to `--until` (today).
fn collect_options(args: &CollectArgs, today: NaiveDate) -> Result<CollectOptions> {
    let (since, until) = match args.range {
        Some(preset) => resolve_preset(preset, today),
        None => (
            args.since.unwrap_or(today - Days::new(30)),
            args.until.unwrap_or(today),
        ),
    };
    day_range(&Local, since, until)
}

/// Writes the export to standard output; a reader that went away (`| head`) ends it quietly.
fn write_stdout(content: &[u8]) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    match stdout.write_all(content).and_then(|()| stdout.flush()) {
        Err(err) if err.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        result => result,
    }
}

/// Where the lines for people go: standard output, or standard error while the export goes to
/// standard output. Once its reader went away, nothing more is said.
struct Status {
    out: Box<dyn Write>,
    closed: bool,
}

impl Status {
    fn stdout() -> Self {
        Self {
            out: Box::new(io::stdout()),
            closed: false,
        }
    }

    fn for_output(output: &Output) -> Self {
        match output {
            Output::File(_) => Self::stdout(),
            Output::Stdout => Self {
                out: Box::new(io::stderr()),
                closed: false,
            },
        }
    }

    fn say(&mut self, line: &str) -> io::Result<()> {
        if self.closed {
            return Ok(());
        }
        match writeln!(self.out, "{line}") {
            Err(err) if err.kind() == io::ErrorKind::BrokenPipe => {
                self.closed = true;
                Ok(())
            }
            result => result,
        }
    }
}

#[cfg(test)]
mod tests;
