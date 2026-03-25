use std::{
    fs,
    path::{Path, PathBuf},
};

use activity_log_core::{
    collect_activity_detailed,
    model::{CollectOptions, OutputFormat, RangePreset},
    ts::activity_typescript,
    utils::{days_ago, ensure_valid_date, to_markdown, today},
};
use anyhow::{Result, anyhow};
use clap::{Parser, Subcommand};
use dotenvy::dotenv;

#[derive(Debug, Parser)]
#[command(author, version, about)]
struct Cli {
    #[arg(long)]
    since: Option<String>,
    #[arg(long)]
    until: Option<String>,
    #[arg(long)]
    output: Option<String>,
    #[arg(long)]
    range: Option<String>,
    #[arg(long = "git-dirs", value_delimiter = ',')]
    git_dirs: Option<Vec<PathBuf>>,
    #[arg(long, default_value = "json")]
    format: String,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    GenerateTs {
        #[arg(long, default_value = "dashboard/src/app/generated/activity.ts")]
        out: PathBuf,
    },
}

#[tokio::main]
async fn main() {
    dotenv().ok();

    if let Err(err) = run().await {
        eprintln!("Fatal: {err}");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::GenerateTs { out }) => generate_ts(&out),
        None => collect(cli).await,
    }
}

async fn collect(cli: Cli) -> Result<()> {
    let format = parse_format(&cli.format)?;
    let options = build_collect_options(&cli, &format)?;

    println!(
        "\nCollecting activity from {} to {} ...\n",
        options.since, options.until
    );

    let collected = collect_activity_detailed(options.clone()).await?;
    println!(
        "  ✓ git     — {} commits ({} merged with gitlab)",
        collected.git_entries.len(),
        collected.merged_count
    );
    println!(
        "  ✓ gitlab  — {} events ({} push events merged into git)",
        collected.gitlab_entries.len(),
        collected.merged_count
    );
    println!("  ✓ jira    — {} issues", collected.jira_entries.len());
    println!(
        "  ✓ outlook — {} calendar events",
        collected.outlook_entries.len()
    );
    println!(
        "  ✓ system  — {} power events",
        collected.system_entries.len()
    );

    let content = match format {
        OutputFormat::Json => serde_json::to_string_pretty(&collected.log)?,
        OutputFormat::Markdown => to_markdown(&collected.log),
    };

    if let Some(output) = &options.output {
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

fn generate_ts(out: &Path) -> Result<()> {
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent)?;
    }

    // Keep the frontend contract generation explicit so backend builds do not mutate repo files.
    fs::write(out, activity_typescript())?;

    let index_path = out
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("index.ts");
    fs::write(index_path, "export * from './activity'\n")?;
    println!("Generated TypeScript DTOs at {}", out.display());
    Ok(())
}

fn parse_format(value: &str) -> Result<OutputFormat> {
    match value.to_ascii_lowercase().as_str() {
        "json" => Ok(OutputFormat::Json),
        "markdown" => Ok(OutputFormat::Markdown),
        other => Err(anyhow!(
            "Invalid --format \"{other}\". Use json or markdown."
        )),
    }
}

fn build_collect_options(cli: &Cli, format: &OutputFormat) -> Result<CollectOptions> {
    let default_output = match format {
        OutputFormat::Json => "logs/activity_log.json",
        OutputFormat::Markdown => "logs/activity_log.md",
    };

    let (since, until) = if let Some(range) = &cli.range {
        let preset = range
            .parse::<RangePreset>()
            .map_err(|message| anyhow!(message))?;
        activity_log_core::utils::resolve_range(preset)
    } else {
        let since = cli.since.clone().unwrap_or_else(|| days_ago(30));
        let until = cli.until.clone().unwrap_or_else(today);
        if cli.since.is_some() {
            ensure_valid_date(&since, "since")?;
        }
        if cli.until.is_some() {
            ensure_valid_date(&until, "until")?;
        }
        (since, until)
    };

    Ok(CollectOptions {
        since,
        until,
        output: Some(
            cli.output
                .clone()
                .unwrap_or_else(|| default_output.to_string()),
        ),
        git_dirs: cli.git_dirs.clone(),
    })
}

#[cfg(test)]
mod tests;
