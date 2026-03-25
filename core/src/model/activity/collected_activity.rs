use crate::model::{
    activity::activity_log::ActivityLog, git::git_entry::GitEntry,
    gitlab::gitlab_entry::GitLabEntry, jira::jira_entry::JiraEntry,
    outlook::outlook_entry::OutlookEntry, system::system_entry::SystemEntry,
};

#[derive(Debug, Clone)]
pub struct CollectedActivity {
    pub git_entries: Vec<GitEntry>,
    pub gitlab_entries: Vec<GitLabEntry>,
    pub jira_entries: Vec<JiraEntry>,
    pub outlook_entries: Vec<OutlookEntry>,
    pub system_entries: Vec<SystemEntry>,
    pub merged_count: usize,
    pub log: ActivityLog,
}
