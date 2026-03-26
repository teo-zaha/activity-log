use std::{
    fs,
    path::{Path, PathBuf},
};

use activity_log_core::{
    collect_activity_detailed_with_provider_runtimes,
    model::{CollectOptions, CollectedActivity, OutputFormat, ProviderInfo, RangePreset, Source},
    provider::inspect_provider_runtimes,
    utils::{days_ago, parse_date, to_markdown, today},
};
use anyhow::{Result, anyhow};
use clap::Parser;
use dotenvy::dotenv;
use strum::IntoEnumIterator;

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
    collect(cli).await
}

async fn collect(cli: Cli) -> Result<()> {
    let format = parse_format(&cli.format)?;
    let options = build_collect_options(&cli, &format)?;

    println!(
        "\nCollecting activity from {} to {} ...\n",
        options.since, options.until
    );

    let provider_runtimes = inspect_provider_runtimes().await;
    let provider_info = provider_runtimes
        .iter()
        .map(|runtime| runtime.info().clone())
        .collect::<Vec<_>>();
    let collected =
        collect_activity_detailed_with_provider_runtimes(options.clone(), &provider_runtimes)
            .await?;

    for line in provider_summary_lines(&provider_info, &collected) {
        println!("{line}");
    }

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

fn parse_format(value: &str) -> Result<OutputFormat> {
    value
        .parse::<OutputFormat>()
        .map_err(|_| anyhow!("Invalid --format \"{value}\". Use json or markdown."))
}

fn build_collect_options(cli: &Cli, format: &OutputFormat) -> Result<CollectOptions> {
    let default_output = match format {
        OutputFormat::Json => "logs/activity_log.json",
        OutputFormat::Markdown => "logs/activity_log.md",
    };

    let (since, until) = if let Some(range) = &cli.range {
        let preset = range
            .parse::<RangePreset>()
            .map_err(|_| anyhow!("Unknown range: \"{range}\". Use {}", valid_range_presets()))?;
        let (since, until) = activity_log_core::utils::resolve_range(preset);
        (parse_date(&since, "since")?, parse_date(&until, "until")?)
    } else {
        let since = cli.since.clone().unwrap_or_else(|| days_ago(30));
        let until = cli.until.clone().unwrap_or_else(today);
        (parse_date(&since, "since")?, parse_date(&until, "until")?)
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
            if !info.usable {
                let reason = info.reason.as_deref().unwrap_or("unavailable");
                return format!("  - {} — skipped ({reason})", info.id);
            }

            let count = collected.summary.entry_count_for_source(&info.id);
            let detail = match info.id {
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

            format!("  ✓ {}{detail}", info.id)
        })
        .collect()
}

#[cfg(test)]
mod tests;
