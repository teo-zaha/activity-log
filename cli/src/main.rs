use std::{collections::BTreeMap, fs, path::Path};

use activity_log_core::{
    collect_activity_detailed_with_provider_runtimes,
    model::{CollectOptions, CollectedActivity, OutputFormat, ProviderInfo, RangePreset, Source},
    provider::{
        inspect_provider_runtimes_with,
        provider_config::{ConfigSource, ProviderConfigKey, provider_config_value_matches_type},
        provider_registry,
    },
    utils::{days_ago, parse_date, to_markdown, today},
};
use anyhow::{Result, anyhow};
use clap::Parser;
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
    #[arg(long = "set", value_name = "KEY=VALUE")]
    set: Vec<String>,
    #[arg(long, default_value = "json")]
    format: String,
}

#[derive(Debug, Clone)]
struct CliCollectionRequest {
    collect_options: CollectOptions,
    output_path: Option<String>,
    config_overrides: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default)]
struct CliConfigSource {
    overrides: BTreeMap<String, String>,
}

impl ConfigSource for CliConfigSource {
    fn get(&self, key: &str) -> Option<String> {
        if let Some(value) = self.overrides.get(key) {
            return Some(value.clone());
        }

        std::env::var(key).ok()
    }
}

#[tokio::main]
async fn main() {
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
    let request = build_collect_request(&cli, &format)?;

    println!(
        "\nCollecting activity from {} to {} ...\n",
        request.collect_options.since, request.collect_options.until
    );

    let config_source = CliConfigSource {
        overrides: request.config_overrides.clone(),
    };
    let provider_runtimes = inspect_provider_runtimes_with(&config_source).await;
    let provider_info = provider_runtimes
        .iter()
        .map(|runtime| runtime.info().clone())
        .collect::<Vec<_>>();
    let collected = collect_activity_detailed_with_provider_runtimes(
        request.collect_options,
        &provider_runtimes,
    )
    .await?;

    for line in provider_summary_lines(&provider_info, &collected) {
        println!("{line}");
    }

    let content = match format {
        OutputFormat::Json => serde_json::to_string_pretty(&collected.log)?,
        OutputFormat::Markdown => to_markdown(&collected.log),
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

fn build_collect_request(cli: &Cli, format: &OutputFormat) -> Result<CliCollectionRequest> {
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

    Ok(CliCollectionRequest {
        collect_options: CollectOptions { since, until },
        output_path: Some(
            cli.output
                .clone()
                .unwrap_or_else(|| default_output.to_string()),
        ),
        config_overrides: parse_provider_config_overrides(&cli.set)?,
    })
}

fn parse_provider_config_overrides(entries: &[String]) -> Result<BTreeMap<String, String>> {
    let known_requirements = provider_requirements_by_key();
    let mut overrides = BTreeMap::new();

    for entry in entries {
        if entry.matches('=').count() != 1 {
            return Err(anyhow!("Invalid --set \"{entry}\". Use KEY=VALUE."));
        }

        let (key, value) = entry
            .split_once('=')
            .ok_or_else(|| anyhow!("Invalid --set \"{entry}\". Use KEY=VALUE."))?;
        if key.is_empty() {
            return Err(anyhow!("Invalid --set \"{entry}\". Use KEY=VALUE."));
        }

        let requirement = known_requirements.get(key).ok_or_else(|| {
            let allowed = known_requirements
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ");
            anyhow!("Unknown provider config key \"{key}\". Allowed keys: {allowed}")
        })?;

        if !provider_config_value_matches_type(value, requirement.value_type) {
            return Err(anyhow!(
                "Invalid value for \"{key}\": expected {}",
                requirement.value_type
            ));
        }

        overrides.insert(key.to_string(), value.to_string());
    }

    Ok(overrides)
}

fn provider_requirements_by_key() -> BTreeMap<String, ProviderConfigKey> {
    let mut requirements = BTreeMap::new();
    for adapter in provider_registry() {
        for requirement in adapter.config_requirements() {
            requirements
                .entry(requirement.name.to_string())
                .or_insert(*requirement);
        }
    }
    requirements
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
