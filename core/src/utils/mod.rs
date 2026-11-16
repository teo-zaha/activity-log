use chrono::{DateTime, TimeZone, Utc};

use crate::{
    collect::gitlab::commit_to_sha,
    model::{
        ActivityEntry, ActivityLog, GitEntry, GitLabEntry, GitLabType, JiraEntry, MergeResult,
        OutlookEntry, SystemEntry,
    },
};

pub fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

pub fn merge_git_with_gitlab(
    git_entries: &[GitEntry],
    gitlab_entries: &[GitLabEntry],
) -> MergeResult {
    // Only GitLab push events have a commit SHA we can safely correlate with local git history.
    let gitlab_by_commit = gitlab_entries
        .iter()
        .filter(|entry| entry.entry_type == GitLabType::PushedTo)
        .filter_map(|entry| commit_to_sha(entry).map(|sha| (sha.to_string(), entry.clone())))
        .collect::<std::collections::HashMap<_, _>>();

    let merged_git = git_entries
        .iter()
        .cloned()
        .map(|mut entry| {
            if let Some(gitlab_event) = gitlab_by_commit.get(&entry.hash) {
                entry.gitlab_event = Some(gitlab_event.clone());
            }
            entry
        })
        .collect::<Vec<_>>();

    let remaining_gitlab = gitlab_entries
        .iter()
        .filter(|entry| {
            if entry.entry_type != GitLabType::PushedTo {
                return true;
            }
            commit_to_sha(entry)
                .map(|sha| !gitlab_by_commit.contains_key(sha))
                .unwrap_or(true)
        })
        .cloned()
        .collect::<Vec<_>>();

    MergeResult {
        merged_git,
        remaining_gitlab,
    }
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
