//! The Markdown export: entries grouped by day in the reader's time zone.

use chrono::{DateTime, TimeZone};

use crate::model::{
    ActivityEntry, ActivityLog, CalendarEntry, GitEntry, GitLabEntry, JiraEntry, OutlookEntry,
    SystemEntry,
};

#[cfg(test)]
mod tests;

/// Renders `log` grouped by calendar day in `tz`, the reader's timezone.
pub fn to_markdown<Tz: TimeZone>(log: &ActivityLog, tz: &Tz) -> String {
    let local = |timestamp: &str| {
        DateTime::parse_from_rfc3339(timestamp)
            .ok()
            .map(|instant| instant.with_timezone(tz).naive_local())
    };
    let range = |timestamp: &str| {
        local(timestamp)
            .map(|instant| instant.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| text(timestamp))
    };
    let mut markdown = format!(
        "# Activity Summary — {} to {}\n\nTotal: {} events\n\n",
        range(&log.meta.start),
        range(&log.meta.end),
        log.entries.len()
    );

    let mut by_day = std::collections::BTreeMap::<chrono::NaiveDate, Vec<_>>::new();
    for entry in &log.entries {
        let instant = entry.timestamp().instant().with_timezone(tz).naive_local();
        by_day
            .entry(instant.date())
            .or_default()
            .push((instant, entry));
    }

    for (day, entries) in by_day.into_iter().rev() {
        let label = day.format("%A, %-d %B %Y");
        markdown.push_str(&format!("## {label}\n\n"));

        for (instant, entry) in entries {
            let time = instant.format("%H:%M");
            match entry {
                ActivityEntry::Git(GitEntry { repo, message, .. }) => {
                    markdown.push_str(&format!(
                        "- **[git]** {} {} ({time})\n",
                        code(repo),
                        text(message)
                    ));
                }
                ActivityEntry::GitLab(GitLabEntry {
                    entry_type, title, ..
                }) => {
                    let title = title
                        .as_deref()
                        .map(|title| format!(": {}", text(title)))
                        .unwrap_or_default();
                    markdown.push_str(&format!("- **[gitlab]** {entry_type}{title} ({time})\n"));
                }
                ActivityEntry::Jira(JiraEntry {
                    key, title, status, ..
                }) => {
                    markdown.push_str(&format!(
                        "- **[jira]** {}: {} [{}] ({time})\n",
                        text(key),
                        text(title),
                        text(status)
                    ));
                }
                ActivityEntry::Outlook(OutlookEntry {
                    title,
                    location,
                    meeting,
                    ..
                }) => {
                    markdown.push_str(&format!(
                        "- **[outlook]** {}{} ({time}, {})\n",
                        text(title.as_deref().unwrap_or(NO_TITLE)),
                        location
                            .as_ref()
                            .map(|location| format!(" @ {}", text(location)))
                            .unwrap_or_default(),
                        format_duration(meeting.duration_minutes),
                    ));
                }
                ActivityEntry::System(SystemEntry { label, .. }) => {
                    markdown.push_str(&format!("- **[system]** {} ({time})\n", text(label)));
                }
                ActivityEntry::Calendar(CalendarEntry {
                    title,
                    location,
                    meeting,
                    ..
                }) => {
                    markdown.push_str(&format!(
                        "- **[calendar]** {}{} ({time}, {})\n",
                        text(title.as_deref().unwrap_or(NO_TITLE)),
                        location
                            .as_ref()
                            .map(|location| format!(" @ {}", text(location)))
                            .unwrap_or_default(),
                        format_duration(meeting.duration_minutes),
                    ));
                }
            }
        }

        markdown.push('\n');
    }

    markdown
}

/// Text from a source on one line, with the characters that would start Markdown inline
/// syntax (emphasis, links, code, HTML, entities) escaped. Line breaks become spaces, so no
/// title or message can start a heading, a list or a block of its own. Kept apart from the
/// terminal's sanitizing: this is about the document's structure.
fn text(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for c in one_line(raw).chars() {
        if matches!(
            c,
            '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '>' | '&' | '~' | '|'
        ) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// `raw` as a code span: fenced by more backticks than any run inside it, and padded where
/// CommonMark would take a character off (it strips one space on each side when both sides
/// have one and the span is not all spaces, and a fence next to a backtick would grow). An
/// empty text is no span at all: two backticks would show as such.
fn code(raw: &str) -> String {
    let raw = one_line(raw);
    if raw.is_empty() {
        return String::new();
    }
    let longest_run = raw.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    let fence = "`".repeat(longest_run + 1);
    // CommonMark counts ASCII spaces only.
    let stripped = raw.starts_with(' ') && raw.ends_with(' ') && raw.bytes().any(|b| b != b' ');
    let pad = if raw.starts_with('`') || raw.ends_with('`') || stripped {
        " "
    } else {
        ""
    };
    format!("{fence}{pad}{raw}{pad}{fence}")
}

/// `raw` with every line break and other control character as a space.
fn one_line(raw: &str) -> String {
    raw.split(|c: char| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}'))
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// What a meeting without a subject is called.
const NO_TITLE: &str = "(No title)";

pub fn format_duration(duration_minutes: i32) -> String {
    if duration_minutes >= 60 {
        let hours = duration_minutes / 60;
        let minutes = duration_minutes % 60;
        if minutes == 0 {
            format!("{hours}h")
        } else {
            format!("{hours}h {minutes}m")
        }
    } else {
        format!("{duration_minutes}m")
    }
}
