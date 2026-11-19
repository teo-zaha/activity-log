//! Activity written to a file, the same for the CLI, the desktop app and the API: JSON is the
//! `ActivityLog`, Markdown is `utils::to_markdown` grouped by day in the reader's time zone.

use std::collections::BTreeMap;

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Duration, TimeZone};
use chrono_tz::Tz;

use crate::{
    model::{
        ActivityEntry, ActivityLog, ActivityLogMeta, CollectRequest, CollectedActivitySummary,
        ConnectionCount, ConnectionId, ExportFile, ExportRequest, OutputFormat,
    },
    runtime::resolve_collect_options,
    utils::{now_iso, to_markdown},
};

#[cfg(test)]
mod tests;

/// `log` as a file of `format`, with days and times in `tz`.
pub fn render_export<Z: TimeZone>(log: &ActivityLog, format: OutputFormat, tz: &Z) -> ExportFile {
    let content = match format {
        OutputFormat::Json => {
            serde_json::to_string_pretty(log).expect("an activity log serializes")
        }
        OutputFormat::Markdown => to_markdown(log, tz),
    };
    ExportFile {
        file_name: file_name(&log.meta, format, tz),
        content,
    }
}

/// The entries of `request` as a file: the range must be valid and the zone an IANA name.
pub fn render_export_request(request: &ExportRequest) -> Result<ExportFile> {
    let tz = request
        .time_zone
        .parse::<Tz>()
        .map_err(|_| anyhow!("Unknown time zone \"{}\"", request.time_zone))?;
    let options = resolve_collect_options(&CollectRequest {
        start: request.start.clone(),
        end: request.end.clone(),
    })
    .map_err(|message| anyhow!(message))
    .context("Invalid export range")?;
    let log = ActivityLog {
        meta: ActivityLogMeta {
            generated: now_iso(),
            start: options.start.to_rfc3339(),
            end: options.end.to_rfc3339(),
            pending: Vec::new(),
        },
        summary: summary_of(&request.entries),
        entries: request.entries.clone(),
    };
    Ok(render_export(&log, request.format, &tz))
}

/// `activity-2026-11-16-2026-11-22.md`: the first and last local day of `[start, end)`.
fn file_name<Z: TimeZone>(meta: &ActivityLogMeta, format: OutputFormat, tz: &Z) -> String {
    let day = |instant: Option<DateTime<_>>| {
        instant.map_or_else(
            || "all".to_string(),
            |instant| instant.with_timezone(tz).date_naive().to_string(),
        )
    };
    let start = DateTime::parse_from_rfc3339(&meta.start).ok();
    let end = DateTime::parse_from_rfc3339(&meta.end).ok();
    let last = match (start, end) {
        (Some(start), Some(end)) if end > start => Some(end - Duration::nanoseconds(1)),
        _ => start,
    };
    format!(
        "activity-{}-{}.{}",
        day(start),
        day(last),
        format.extension()
    )
}

/// What each connection contributed among `entries`, and how many commits absorbed a push.
fn summary_of(entries: &[ActivityEntry]) -> CollectedActivitySummary {
    let mut counts = BTreeMap::<ConnectionId, usize>::new();
    for entry in entries {
        *counts
            .entry(ConnectionId::new(entry.source(), entry.connection()))
            .or_default() += 1;
    }
    CollectedActivitySummary {
        connections: counts
            .into_iter()
            .map(|(connection, count)| ConnectionCount { connection, count })
            .collect(),
        merged_count: entries
            .iter()
            .filter(|entry| matches!(entry, ActivityEntry::Git(git) if git.gitlab_event.is_some()))
            .count(),
    }
}
