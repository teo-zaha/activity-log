use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::{
    git::git_entry::GitEntry, gitlab::gitlab_entry::GitLabEntry, jira::jira_entry::JiraEntry,
    outlook::outlook_entry::OutlookEntry, system::system_entry::SystemEntry,
};

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(untagged)]
#[ts(export, untagged)]
pub enum ActivityEntry {
    Git(GitEntry),
    GitLab(GitLabEntry),
    Jira(JiraEntry),
    Outlook(OutlookEntry),
    System(SystemEntry),
}

pub trait IntoActivityEntry {
    fn into_activity_entry(self) -> ActivityEntry;
}

impl IntoActivityEntry for GitEntry {
    fn into_activity_entry(self) -> ActivityEntry {
        ActivityEntry::Git(self)
    }
}

impl IntoActivityEntry for GitLabEntry {
    fn into_activity_entry(self) -> ActivityEntry {
        ActivityEntry::GitLab(self)
    }
}

impl IntoActivityEntry for JiraEntry {
    fn into_activity_entry(self) -> ActivityEntry {
        ActivityEntry::Jira(self)
    }
}

impl IntoActivityEntry for OutlookEntry {
    fn into_activity_entry(self) -> ActivityEntry {
        ActivityEntry::Outlook(self)
    }
}

impl IntoActivityEntry for SystemEntry {
    fn into_activity_entry(self) -> ActivityEntry {
        ActivityEntry::System(self)
    }
}

pub trait IntoActivityEntries: IntoIterator
where
    Self::Item: IntoActivityEntry,
{
    fn into_activity_entries(self) -> Vec<ActivityEntry>
    where
        Self: Sized,
    {
        self.into_iter()
            .map(IntoActivityEntry::into_activity_entry)
            .collect()
    }
}

impl<T> IntoActivityEntries for T
where
    T: IntoIterator,
    T::Item: IntoActivityEntry,
{
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
