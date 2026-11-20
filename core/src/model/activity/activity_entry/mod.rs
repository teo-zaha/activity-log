use serde::{Deserialize, Deserializer, Serialize, de::Error as _};
use serde_json::Value;
use ts_rs::TS;

use crate::model::{
    ConnectionStamp, MeetingBody, Timestamp, calendar::calendar_entry::CalendarEntry,
    common::source::Source, git::git_entry::GitEntry, gitlab::gitlab_entry::GitLabEntry,
    jira::jira_entry::JiraEntry, outlook::outlook_entry::OutlookEntry,
    system::system_entry::SystemEntry,
};

/// An entry of any kind, sent as its kind's object; its `source` says which. Read by that
/// `source`: an object is read as the kind it names, with that kind's fields, or refused.
#[derive(Debug, Clone, Serialize, TS)]
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

impl<'de> Deserialize<'de> for ActivityEntry {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        let source = value
            .get("source")
            .ok_or_else(|| D::Error::missing_field("source"))?;
        let source = Source::deserialize(source).map_err(D::Error::custom)?;
        let entry = match source {
            Source::Git => serde_json::from_value(value).map(Self::Git),
            Source::Gitlab => serde_json::from_value(value).map(Self::GitLab),
            Source::Jira => serde_json::from_value(value).map(Self::Jira),
            Source::Outlook => serde_json::from_value(value).map(Self::Outlook),
            Source::System => serde_json::from_value(value).map(Self::System),
            Source::Calendar => serde_json::from_value(value).map(Self::Calendar),
        };
        entry.map_err(|err| D::Error::custom(format!("{source} entry: {err}")))
    }
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

    /// The entry's connection, once the runtime stamped it.
    pub fn connection(&self) -> Option<&str> {
        match self {
            ActivityEntry::Git(entry) => entry.connection.as_str(),
            ActivityEntry::GitLab(entry) => entry.connection.as_str(),
            ActivityEntry::Jira(entry) => entry.connection.as_str(),
            ActivityEntry::Outlook(entry) => entry.connection.as_str(),
            ActivityEntry::System(entry) => entry.connection.as_str(),
            ActivityEntry::Calendar(entry) => entry.connection.as_str(),
        }
    }

    pub fn set_connection(&mut self, name: &ConnectionStamp) {
        let connection = match self {
            ActivityEntry::Git(entry) => &mut entry.connection,
            ActivityEntry::GitLab(entry) => &mut entry.connection,
            ActivityEntry::Jira(entry) => &mut entry.connection,
            ActivityEntry::Outlook(entry) => &mut entry.connection,
            ActivityEntry::System(entry) => &mut entry.connection,
            ActivityEntry::Calendar(entry) => &mut entry.connection,
        };
        connection.stamp(name);
    }

    pub fn id(&self) -> &str {
        match self {
            ActivityEntry::Git(entry) => &entry.id,
            ActivityEntry::GitLab(entry) => &entry.id,
            ActivityEntry::Jira(entry) => &entry.id,
            ActivityEntry::Outlook(entry) => &entry.id,
            ActivityEntry::System(entry) => &entry.id,
            ActivityEntry::Calendar(entry) => &entry.id,
        }
    }

    pub fn id_mut(&mut self) -> &mut String {
        match self {
            ActivityEntry::Git(entry) => &mut entry.id,
            ActivityEntry::GitLab(entry) => &mut entry.id,
            ActivityEntry::Jira(entry) => &mut entry.id,
            ActivityEntry::Outlook(entry) => &mut entry.id,
            ActivityEntry::System(entry) => &mut entry.id,
            ActivityEntry::Calendar(entry) => &mut entry.id,
        }
    }

    /// The meeting part of a meeting entry.
    pub fn meeting(&self) -> Option<&MeetingBody> {
        match self {
            ActivityEntry::Outlook(entry) => Some(&entry.meeting),
            ActivityEntry::Calendar(entry) => Some(&entry.meeting),
            _ => None,
        }
    }

    pub fn timestamp(&self) -> &Timestamp {
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

#[cfg(test)]
mod tests;
