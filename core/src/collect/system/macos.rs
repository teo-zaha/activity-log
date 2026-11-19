#[cfg(target_os = "macos")]
use anyhow::{Result, anyhow};
use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
#[cfg(target_os = "macos")]
use tokio::process::Command;

#[cfg(target_os = "macos")]
use crate::model::CollectOptions;
use crate::model::{SystemEntry, SystemEntryType};

const BOOT_LABEL: &str = "System started";
const SHUTDOWN_LABEL: &str = "System shutdown";

/// Boot and shutdown from `last`, sleep and wake from the power log, read at the same time. The
/// power log only adds to what `last` has: when it cannot be read, that is logged and the boots
/// and shutdowns stay.
#[cfg(target_os = "macos")]
pub(super) async fn collect_system(options: &CollectOptions) -> Result<Vec<SystemEntry>> {
    let (boots, power) = tokio::join!(collect_last(options), collect_power_log(options));
    let mut entries = boots?;
    match power {
        Ok(power) => entries.extend(power),
        Err(err) => crate::diagnostics::warn(
            "system",
            format!("Sleep and wake are left out: reading the power log failed: {err:#}"),
        ),
    }
    Ok(entries)
}

/// `pmset -g log`, oldest first and only about the last week (macOS rotates it). Streamed: the
/// lines before the range are passed over without parsing a time, and reading stops at the
/// first sleep or wake after it.
#[cfg(target_os = "macos")]
async fn collect_power_log(options: &CollectOptions) -> Result<Vec<SystemEntry>> {
    use std::process::Stdio;
    use tokio::io::{AsyncBufReadExt, BufReader};

    let mut child = Command::new("/usr/bin/pmset")
        .args(["-g", "log"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("pmset produced no output stream"))?;
    let mut lines = BufReader::new(stdout).lines();
    let mut entries = Vec::new();
    while let Some(line) = lines.next_line().await? {
        match classify_power_line(&line, options.start, options.end) {
            PowerLine::Entry(entry) => entries.push(entry),
            PowerLine::Skip => {}
            PowerLine::AfterRange => return Ok(entries),
        }
    }
    let status = child.wait().await?;
    if !status.success() {
        return Err(anyhow!("pmset exited {status}"));
    }
    Ok(entries)
}

#[cfg(target_os = "macos")]
async fn collect_last(options: &CollectOptions) -> Result<Vec<SystemEntry>> {
    use std::process::Stdio;
    use tokio::io::{AsyncBufReadExt, BufReader};

    let mut reader = LastReader::new(options.start, options.end, chrono::Local);
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
    let mut before_range = false;
    while let Some(line) = lines.next_line().await? {
        match reader.line(&line) {
            LastLine::Entry(entry) => entries.push(entry),
            LastLine::Skip => {}
            LastLine::Unreadable(time) => crate::diagnostics::warn(
                "system",
                format!("Skipped a boot or shutdown at an unreadable local time: {time}"),
            ),
            LastLine::BeforeRange => {
                before_range = true;
                break;
            }
        }
    }
    if let Some(first) = reader.undecided().first() {
        crate::diagnostics::warn(
            "system",
            format!(
                "{} boots or shutdowns fall in an hour a clock change repeats and could be either time; taken as the later (first: {first})",
                reader.undecided().len()
            ),
        );
    }
    // Stopped early: dropping the child (kill_on_drop) ends it.
    if before_range {
        return Ok(entries);
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
    /// A boot or shutdown whose time cannot be read, or names no time in `zone` (inside a
    /// clock change's gap): left out, the rest of the log stays.
    Unreadable(String),
    /// Older than the range; everything after it is older too.
    BeforeRange,
}

/// Reads `last`'s lines, newest first, their local times in `zone`.
pub(crate) struct LastReader<Z: TimeZone> {
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    zone: Z,
    /// The time of the record before, which is not older than this one.
    newer: Option<DateTime<Utc>>,
    /// Records in a repeated hour whose time neither the range nor the order decided.
    undecided: Vec<String>,
}

impl<Z: TimeZone> LastReader<Z> {
    pub(crate) fn new(start: DateTime<Utc>, end: DateTime<Utc>, zone: Z) -> Self {
        Self {
            start,
            end,
            zone,
            newer: None,
            undecided: Vec::new(),
        }
    }

    /// The records in an hour a clock change repeats whose time could be either reading; each
    /// was taken as the later one.
    pub(crate) fn undecided(&self) -> &[String] {
        &self.undecided
    }

    pub(crate) fn line(&mut self, line: &str) -> LastLine {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("wtmp begins") {
            return LastLine::Skip;
        }
        let Some((entry_type, label, raw_ts)) = parse_last_line(trimmed) else {
            return LastLine::Skip;
        };
        let Some((earliest, latest)) = parse_last_timestamp(raw_ts, &self.zone) else {
            return LastLine::Unreadable(raw_ts.to_string());
        };
        // In an hour a clock change repeats: the reading in the range if only one is, else
        // the later reading unless that would put it after the newer record before it.
        let in_range = |at: DateTime<Utc>| self.start <= at && at < self.end;
        let timestamp = match (in_range(earliest), in_range(latest)) {
            _ if earliest == latest => latest,
            (true, false) => earliest,
            (false, true) => latest,
            (true, true) if self.newer.is_some_and(|newer| latest > newer) => earliest,
            (true, true) => {
                self.undecided.push(raw_ts.to_string());
                latest
            }
            (false, false) if self.newer.is_some_and(|newer| latest > newer) => earliest,
            (false, false) => latest,
        };
        self.newer = Some(timestamp);
        if latest < self.start {
            return LastLine::BeforeRange;
        }
        if timestamp < self.start || timestamp >= self.end {
            return LastLine::Skip;
        }
        LastLine::Entry(SystemEntry {
            source: "system".to_string(),
            connection: String::new(),
            id: format!("{entry_type}/{}", timestamp.to_rfc3339()),
            entry_type,
            timestamp: timestamp.to_rfc3339(),
            label: label.to_string(),
        })
    }
}

/// Same rules as the streaming collector, for captured `last` output.
#[cfg(test)]
pub(crate) fn parse_last_output<Z: TimeZone>(
    stdout: String,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    zone: Z,
) -> Vec<SystemEntry> {
    let mut reader = LastReader::new(start, end, zone);
    let mut entries = Vec::new();
    for line in stdout.lines() {
        match reader.line(line) {
            LastLine::Entry(entry) => entries.push(entry),
            LastLine::Skip | LastLine::Unreadable(_) => {}
            LastLine::BeforeRange => break,
        }
    }
    entries
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

/// `last` prints the machine's local wall-clock time without an offset, so the collector reads
/// it in the local time zone: only the machine knows what it meant. The earliest and latest
/// instant it can mean, which differ in an hour a clock change repeats; `None` for no time or
/// one a clock change skips.
fn parse_last_timestamp<Z: TimeZone>(
    raw: &str,
    zone: &Z,
) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
    let timestamp = NaiveDateTime::parse_from_str(raw, "%a %b %e %Y %H:%M").ok()?;
    let local = zone.from_local_datetime(&timestamp);
    let latest = local.clone().latest()?.with_timezone(&Utc);
    Some((local.earliest()?.with_timezone(&Utc), latest))
}

pub(crate) enum PowerLine {
    Entry(SystemEntry),
    Skip,
    /// A sleep or wake at or after the range's end; the log is oldest first.
    AfterRange,
}

/// Sleep reasons that are the Mac's own business: maintenance (Power Nap, backups) and going
/// back to sleep after a dark wake. Every other `Entering Sleep state` line is a sleep someone
/// would recognise; a `Sleep` line that enters `DarkWake` is not a sleep.
const NOT_A_SLEEP: &[&str] = &["Maintenance Sleep", "Back to Sleep"];

/// One line of `pmset -g log`: `2026-11-16 09:12:03 +0100 Sleep  <tab>Entering Sleep state due to
/// 'Clamshell Sleep':…`. Its type column is padded; only `Sleep` and `Wake` (a full wake, not
/// `DarkWake`, `Wake Requests`, `WakeDetails` or `WakeTime`) are entries. The time carries its
/// offset, so no time zone is read.
pub(crate) fn classify_power_line(
    line: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> PowerLine {
    const TIME_LEN: usize = "2026-11-16 09:12:03 +0100".len();
    let (Some(raw_time), Some(rest)) = (line.get(..TIME_LEN), line.get(TIME_LEN..)) else {
        return PowerLine::Skip;
    };
    let (kind, details) = rest.split_once('\t').unwrap_or((rest, ""));
    let entry = match kind.trim() {
        "Sleep" => sleep_label(details).map(|label| (SystemEntryType::Sleep, label)),
        "Wake" => Some((SystemEntryType::Wake, wake_label(details))),
        _ => None,
    };
    let Some((entry_type, label)) = entry else {
        return PowerLine::Skip;
    };
    let Ok(at) = DateTime::parse_from_str(raw_time, "%Y-%m-%d %H:%M:%S %z") else {
        return PowerLine::Skip;
    };
    let at = at.with_timezone(&Utc);
    if at >= end {
        return PowerLine::AfterRange;
    }
    if at < start {
        return PowerLine::Skip;
    }
    PowerLine::Entry(SystemEntry {
        source: "system".to_string(),
        connection: String::new(),
        id: format!("{entry_type}/{}", at.to_rfc3339()),
        entry_type,
        timestamp: at.to_rfc3339(),
        label,
    })
}

/// `Sleep (lid closed)` for a sleep the user would recognise, `None` for the Mac's own.
fn sleep_label(details: &str) -> Option<String> {
    let details = details.trim_start();
    if !details.starts_with("Entering Sleep state") {
        return None;
    }
    let reason = details
        .split_once("due to '")
        .and_then(|(_, rest)| rest.split_once('\''))
        .map(|(reason, _)| reason);
    if reason.is_some_and(|reason| NOT_A_SLEEP.iter().any(|skip| reason.contains(skip))) {
        return None;
    }
    // Reasons can carry details after their name (`Software Sleep pid=123`).
    const KNOWN: &[(&str, &str)] = &[
        ("Clamshell Sleep", "lid closed"),
        ("Idle Sleep", "idle"),
        ("Software Sleep", "requested"),
        ("Power Button", "power button"),
        ("Low Power Sleep", "low battery"),
    ];
    let why = reason.map(|reason| {
        KNOWN
            .iter()
            .find(|(name, _)| reason.starts_with(name))
            .map_or_else(
                || reason.trim_end_matches(" Sleep").to_lowercase(),
                |(_, why)| why.to_string(),
            )
    });
    Some(with_reason("Sleep", why.as_deref()))
}

/// `Wake (lid opened)`: what the log names after `due to`, where it says something a person
/// did.
fn wake_label(details: &str) -> String {
    let cause = details.split_once("due to").map_or("", |(_, cause)| cause);
    let words = cause.split_whitespace().collect::<Vec<_>>();
    let why = if words.contains(&"lid") {
        Some("lid opened")
    } else if words.contains(&"pwrbtn") {
        Some("power button")
    } else if cause.contains("UserActivity") {
        Some("user activity")
    } else if cause.contains("HID Activity") {
        Some("keyboard or mouse")
    } else if words.first() == Some(&"Notification") {
        Some("notification")
    } else {
        None
    };
    with_reason("Wake", why)
}

fn with_reason(what: &str, why: Option<&str>) -> String {
    match why {
        Some(why) => format!("{what} ({why})"),
        None => what.to_string(),
    }
}

/// Same rules as the streaming collector, for captured `pmset -g log` output.
#[cfg(test)]
pub(crate) fn parse_power_log(
    stdout: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Vec<SystemEntry> {
    let mut entries = Vec::new();
    for line in stdout.lines() {
        match classify_power_line(line, start, end) {
            PowerLine::Entry(entry) => entries.push(entry),
            PowerLine::Skip => {}
            PowerLine::AfterRange => break,
        }
    }
    entries
}
