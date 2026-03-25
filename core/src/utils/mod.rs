use anyhow::{Result, anyhow};
use chrono::{Datelike, Duration, NaiveDate, Utc};

use crate::{
    collect::gitlab::commit_to_sha,
    model::{
        ActivityEntry, ActivityLog, GitEntry, GitLabEntry, GitLabType, JiraEntry, MergeResult,
        OutlookEntry, RangePreset, SystemEntry,
    },
};

pub fn ensure_valid_date(value: &str, name: &str) -> Result<()> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map(|_| ())
        .map_err(|_| anyhow!("Invalid date for {name}: {value:?} (expected YYYY-MM-DD)"))
}

pub fn today() -> String {
    Utc::now().date_naive().format("%Y-%m-%d").to_string()
}

pub fn days_ago(days: i64) -> String {
    (Utc::now().date_naive() - Duration::days(days))
        .format("%Y-%m-%d")
        .to_string()
}

pub fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

pub fn resolve_range(range: RangePreset) -> (String, String) {
    let now = Utc::now().date_naive();
    let fmt = |date: NaiveDate| date.format("%Y-%m-%d").to_string();

    match range {
        RangePreset::Today => (fmt(now), fmt(now)),
        RangePreset::Yesterday => {
            let value = now - Duration::days(1);
            (fmt(value), fmt(value))
        }
        RangePreset::Week => {
            let start = now - Duration::days(now.weekday().num_days_from_sunday() as i64);
            (fmt(start), fmt(now))
        }
        RangePreset::LastWeek => {
            let end = now - Duration::days(now.weekday().num_days_from_sunday() as i64 + 1);
            let start = end - Duration::days(6);
            (fmt(start), fmt(end))
        }
        RangePreset::Month => {
            let start = NaiveDate::from_ymd_opt(now.year(), now.month(), 1).unwrap();
            (fmt(start), fmt(now))
        }
        RangePreset::LastMonth => {
            let start = if now.month() == 1 {
                NaiveDate::from_ymd_opt(now.year() - 1, 12, 1).unwrap()
            } else {
                NaiveDate::from_ymd_opt(now.year(), now.month() - 1, 1).unwrap()
            };
            let end =
                NaiveDate::from_ymd_opt(now.year(), now.month(), 1).unwrap() - Duration::days(1);
            (fmt(start), fmt(end))
        }
    }
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

pub fn to_markdown(log: &ActivityLog) -> String {
    let mut markdown = format!(
        "# Activity Summary — {} to {}\n\nTotal: {} events\n\n",
        log.meta.since,
        log.meta.until,
        log.entries.len()
    );

    let mut by_day = std::collections::BTreeMap::<String, Vec<&ActivityEntry>>::new();
    for entry in &log.entries {
        let day = entry
            .timestamp()
            .get(0..10)
            .unwrap_or("unknown")
            .to_string();
        by_day.entry(day).or_default().push(entry);
    }

    for (day, entries) in by_day.into_iter().rev() {
        let label = NaiveDate::parse_from_str(&day, "%Y-%m-%d")
            .map(|date| date.format("%A, %-d %B %Y").to_string())
            .unwrap_or(day.clone());
        markdown.push_str(&format!("## {label}\n\n"));

        for entry in entries {
            let time = entry.timestamp().get(11..16).unwrap_or_default();
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
