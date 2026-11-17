use anyhow::{Result, anyhow};
use chrono::{DateTime, Local, NaiveDateTime, TimeZone, Utc};
#[cfg(target_os = "macos")]
use tokio::process::Command;

#[cfg(target_os = "macos")]
use crate::model::CollectOptions;
use crate::model::{SystemEntry, SystemEntryType};

const BOOT_LABEL: &str = "System started";
const SHUTDOWN_LABEL: &str = "System shutdown";

#[cfg(target_os = "macos")]
pub(super) async fn collect_system(options: &CollectOptions) -> Result<Vec<SystemEntry>> {
    use std::process::Stdio;
    use tokio::io::{AsyncBufReadExt, BufReader};

    let (start, end) = (options.start, options.end);
    // `last` prints newest first and covers the whole wtmp history. Stream it and stop at the
    // first entry older than the range instead of reading everything; dropping the child
    // (kill_on_drop) ends the process early.
    let mut child = Command::new("/usr/bin/last")
        .arg("-y")
        .arg("reboot")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("last produced no output stream"))?;

    let mut lines = BufReader::new(stdout).lines();
    let mut entries = Vec::new();
    while let Some(line) = lines.next_line().await? {
        match classify_last_line(&line, start, end)? {
            LastLine::Entry(entry) => entries.push(entry),
            LastLine::Skip => {}
            LastLine::BeforeRange => return Ok(entries),
        }
    }

    let output = child.wait_with_output().await?;
    if !output.status.success() {
        return Err(anyhow!(
            "last exited {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(entries)
}

pub(crate) enum LastLine {
    Entry(SystemEntry),
    Skip,
    /// Older than the range; everything after it is older too.
    BeforeRange,
}

pub(crate) fn classify_last_line(
    line: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<LastLine> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with("wtmp begins") {
        return Ok(LastLine::Skip);
    }
    let Some((entry_type, label, raw_ts)) = parse_last_line(trimmed) else {
        return Ok(LastLine::Skip);
    };
    let timestamp = parse_last_timestamp(raw_ts)?;
    if timestamp < start {
        return Ok(LastLine::BeforeRange);
    }
    if timestamp >= end {
        return Ok(LastLine::Skip);
    }
    Ok(LastLine::Entry(SystemEntry {
        source: "system".to_string(),
        connection: String::new(),
        entry_type,
        timestamp: timestamp.to_rfc3339(),
        label: label.to_string(),
    }))
}

/// Same rules as the streaming collector, for captured `last` output.
#[cfg(test)]
pub(crate) fn parse_last_output(
    stdout: String,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<Vec<SystemEntry>> {
    let mut entries = Vec::new();
    for line in stdout.lines() {
        match classify_last_line(line, start, end)? {
            LastLine::Entry(entry) => entries.push(entry),
            LastLine::Skip => {}
            LastLine::BeforeRange => break,
        }
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

/// `last` prints the machine's local wall-clock time without an offset, so this is the one
/// place the backend reads the local timezone: only the machine knows what it meant.
fn parse_last_timestamp(raw: &str) -> Result<DateTime<Utc>> {
    let timestamp = NaiveDateTime::parse_from_str(raw, "%a %b %e %Y %H:%M")?;
    Ok(Local
        .from_local_datetime(&timestamp)
        .single()
        .ok_or_else(|| anyhow!("invalid last timestamp: {raw}"))?
        .with_timezone(&Utc))
}
