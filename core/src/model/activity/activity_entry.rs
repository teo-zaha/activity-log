use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::{
    calendar::calendar_entry::CalendarEntry, common::source::Source, git::git_entry::GitEntry,
    gitlab::gitlab_entry::GitLabEntry, jira::jira_entry::JiraEntry,
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
    Calendar(CalendarEntry),
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

impl IntoActivityEntry for CalendarEntry {
    fn into_activity_entry(self) -> ActivityEntry {
        ActivityEntry::Calendar(self)
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
    pub fn source(&self) -> Source {
        match self {
            ActivityEntry::Git(_) => Source::Git,
            ActivityEntry::GitLab(_) => Source::Gitlab,
            ActivityEntry::Jira(_) => Source::Jira,
            ActivityEntry::Outlook(_) => Source::Outlook,
            ActivityEntry::System(_) => Source::System,
            ActivityEntry::Calendar(_) => Source::Calendar,
        }
    }

    pub fn connection(&self) -> &str {
        match self {
            ActivityEntry::Git(entry) => &entry.connection,
            ActivityEntry::GitLab(entry) => &entry.connection,
            ActivityEntry::Jira(entry) => &entry.connection,
            ActivityEntry::Outlook(entry) => &entry.connection,
            ActivityEntry::System(entry) => &entry.connection,
            ActivityEntry::Calendar(entry) => &entry.connection,
        }
    }

    pub fn set_connection(&mut self, name: &str) {
        let connection = match self {
            ActivityEntry::Git(entry) => &mut entry.connection,
            ActivityEntry::GitLab(entry) => &mut entry.connection,
            ActivityEntry::Jira(entry) => &mut entry.connection,
            ActivityEntry::Outlook(entry) => &mut entry.connection,
            ActivityEntry::System(entry) => &mut entry.connection,
            ActivityEntry::Calendar(entry) => &mut entry.connection,
        };
        name.clone_into(connection);
    }

    pub fn timestamp(&self) -> &str {
        match self {
            ActivityEntry::Git(entry) => &entry.timestamp,
            ActivityEntry::GitLab(entry) => &entry.timestamp,
            ActivityEntry::Jira(entry) => &entry.timestamp,
            ActivityEntry::Outlook(entry) => &entry.timestamp,
            ActivityEntry::System(entry) => &entry.timestamp,
            ActivityEntry::Calendar(entry) => &entry.timestamp,
        }
    }
}
