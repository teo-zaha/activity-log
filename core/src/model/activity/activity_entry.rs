use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::{
    git::git_entry::GitEntry, gitlab::gitlab_entry::GitLabEntry, jira::jira_entry::JiraEntry,
    outlook::outlook_entry::OutlookEntry, system::system_entry::SystemEntry,
};

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(untagged)]
#[ts(export)]
pub enum ActivityEntry {
    Git(GitEntry),
    GitLab(GitLabEntry),
    Jira(JiraEntry),
    Outlook(OutlookEntry),
    System(SystemEntry),
}

impl ActivityEntry {
    pub fn timestamp(&self) -> &str {
        match self {
            ActivityEntry::Git(entry) => &entry.timestamp,
            ActivityEntry::GitLab(entry) => &entry.timestamp,
            ActivityEntry::Jira(entry) => &entry.timestamp,
            ActivityEntry::Outlook(entry) => &entry.timestamp,
            ActivityEntry::System(entry) => &entry.timestamp,
        }
    }
}
