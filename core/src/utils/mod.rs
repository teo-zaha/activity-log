use chrono::{DateTime, TimeZone, Utc};

use crate::model::{
    ActivityEntry, ActivityLog, CalendarEntry, GitEntry, GitLabEntry, GitLabType, JiraEntry,
    MergeResult, OutlookEntry, SystemEntry,
};

pub fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

/// An id for an entry whose source has none: FNV-1a over `parts`, which stays the same across
/// runs and Rust versions (unlike `DefaultHasher`).
pub fn stable_id(parts: &[&str]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for (index, part) in parts.iter().enumerate() {
        let separator = (index > 0).then_some(0x1f_u8);
        for byte in separator.into_iter().chain(part.bytes()) {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    }
    format!("h:{hash:016x}")
}

/// Folds each GitLab push into the git commit it pushed, so the change shows once. A push whose
/// commit is not among `git_entries` (not collected yet, another author's, no local clone)
/// stays an entry of its own.
pub fn merge_git_with_gitlab(
    git_entries: &[GitEntry],
    gitlab_entries: &[GitLabEntry],
) -> MergeResult {
    let gitlab_by_commit = gitlab_entries
        .iter()
        .filter_map(|entry| push_sha(entry).map(|sha| (sha, entry)))
        .collect::<std::collections::HashMap<_, _>>();

    let mut folded = std::collections::HashSet::new();
    let merged_git = git_entries
        .iter()
        .cloned()
        .map(|mut entry| {
            if let Some((sha, gitlab_event)) = gitlab_by_commit.get_key_value(entry.hash.as_str()) {
                entry.gitlab_event = Some(Box::new((*gitlab_event).clone()));
                folded.insert(*sha);
            }
            entry
        })
        .collect::<Vec<_>>();

    let remaining_gitlab = gitlab_entries
        .iter()
        .filter(|entry| !push_sha(entry).is_some_and(|sha| folded.contains(sha)))
        .cloned()
        .collect::<Vec<_>>();

    MergeResult {
        merged_git,
        remaining_gitlab,
    }
}

/// Only GitLab push events have a commit SHA we can safely correlate with local git history.
fn push_sha(entry: &GitLabEntry) -> Option<&str> {
    (entry.entry_type == GitLabType::PushedTo)
        .then_some(entry.pushed_commit.as_deref())
        .flatten()
}

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
            .unwrap_or_else(|| timestamp.to_string())
    };
    let mut markdown = format!(
        "# Activity Summary — {} to {}\n\nTotal: {} events\n\n",
        range(&log.meta.start),
        range(&log.meta.end),
        log.entries.len()
    );

    let mut by_day = std::collections::BTreeMap::<Option<chrono::NaiveDate>, Vec<_>>::new();
    for entry in &log.entries {
        let instant = local(entry.timestamp());
        by_day
            .entry(instant.map(|instant| instant.date()))
            .or_default()
            .push((instant, entry));
    }

    for (day, entries) in by_day.into_iter().rev() {
        let label = day
            .map(|date| date.format("%A, %-d %B %Y").to_string())
            .unwrap_or_else(|| "Unknown date".to_string());
        markdown.push_str(&format!("## {label}\n\n"));

        for (instant, entry) in entries {
            let time = instant
                .map(|instant| instant.format("%H:%M").to_string())
                .unwrap_or_default();
            match entry {
                ActivityEntry::Git(GitEntry { repo, message, .. }) => {
                    markdown.push_str(&format!("- **[git]** `{repo}` {message} ({time})\n"));
                }
                ActivityEntry::GitLab(GitLabEntry {
                    entry_type, title, ..
                }) => {
                    markdown.push_str(&format!("- **[gitlab]** {entry_type}: {title} ({time})\n"));
                }
                ActivityEntry::Jira(JiraEntry {
                    key, title, status, ..
                }) => {
                    markdown.push_str(&format!(
                        "- **[jira]** {key}: {title} [{status}] ({time})\n"
                    ));
                }
                ActivityEntry::Outlook(OutlookEntry {
                    title,
                    location,
                    duration_minutes,
                    ..
                }) => {
                    markdown.push_str(&format!(
                        "- **[outlook]** {title}{} ({time}, {})\n",
                        location
                            .as_ref()
                            .map(|location| format!(" @ {location}"))
                            .unwrap_or_default(),
                        format_duration(*duration_minutes),
                    ));
                }
                ActivityEntry::System(SystemEntry { label, .. }) => {
                    markdown.push_str(&format!("- **[system]** {label} ({time})\n"));
                }
                ActivityEntry::Calendar(CalendarEntry {
                    title,
                    location,
                    duration_minutes,
                    ..
                }) => {
                    markdown.push_str(&format!(
                        "- **[calendar]** {title}{} ({time}, {})\n",
                        location
                            .as_ref()
                            .map(|location| format!(" @ {location}"))
                            .unwrap_or_default(),
                        format_duration(*duration_minutes),
                    ));
                }
            }
        }

        markdown.push('\n');
    }

    markdown
}

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

#[cfg(test)]
mod tests;
