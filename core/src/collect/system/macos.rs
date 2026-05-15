use anyhow::{Result, anyhow};
use chrono::{DateTime, Local, NaiveDateTime, TimeZone, Utc};
use tokio::process::Command;

use crate::model::{CollectOptions, SystemEntry, SystemEntryType};

use super::utc_bounds;

const BOOT_LABEL: &str = "System started";
const SHUTDOWN_LABEL: &str = "System shutdown";

pub(super) async fn collect_system(options: &CollectOptions) -> Result<Vec<SystemEntry>> {
    let (start, end) = utc_bounds(options.since, options.until)?;
    parse_last_output(last_output().await?, start, end)
}

async fn last_output() -> Result<String> {
    let output = Command::new("/usr/bin/last")
        .arg("-y")
        .arg("reboot")
        .output()
        .await?;

    if !output.status.success() {
        return Err(anyhow!(
            "last exited {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub(crate) fn parse_last_output(
    stdout: String,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<Vec<SystemEntry>> {
    let mut entries = Vec::new();

    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("wtmp begins") {
            continue;
        }

        let Some((entry_type, label, raw_ts)) = parse_last_line(trimmed) else {
            continue;
        };
        let timestamp = parse_last_timestamp(raw_ts)?;
        if timestamp < start || timestamp > end {
            continue;
        }

        entries.push(SystemEntry {
            source: "system".to_string(),
            entry_type,
            timestamp: timestamp.to_rfc3339(),
            label: label.to_string(),
        });
    }

    Ok(entries)
}

fn parse_last_line(line: &str) -> Option<(SystemEntryType, &'static str, &str)> {
    if let Some(rest) = line.strip_prefix("reboot time") {
        return Some((SystemEntryType::Boot, BOOT_LABEL, rest.trim()));
    }

    if let Some(rest) = line.strip_prefix("shutdown time") {
        return Some((SystemEntryType::Shutdown, SHUTDOWN_LABEL, rest.trim()));
    }

    None
}

fn parse_last_timestamp(raw: &str) -> Result<DateTime<Utc>> {
    let timestamp = NaiveDateTime::parse_from_str(raw, "%a %b %e %Y %H:%M")?;
    Ok(Local
        .from_local_datetime(&timestamp)
        .single()
        .ok_or_else(|| anyhow!("invalid last timestamp: {raw}"))?
        .with_timezone(&Utc))
}
