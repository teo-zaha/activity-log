use serde::{Deserialize, Deserializer, Serialize, de::Error as _};
use serde_json::Value;
use ts_rs::TS;

use crate::model::{
    ConnectionStamp, MeetingBody, Timestamp, calendar::calendar_entry::CalendarEntry,
    common::source::Source, git::git_entry::GitEntry, github::GitHubEntry,
    gitlab::gitlab_entry::GitLabEntry, jira::jira_entry::JiraEntry,
    outlook::outlook_entry::OutlookEntry, system::system_entry::SystemEntry,
};

pub trait IntoActivityEntry {
    fn into_activity_entry(self) -> ActivityEntry;
}

macro_rules! activity_entry {
    ($(#[$doc:meta])* $($variant:ident($entry:ty) => $source:ident),+ $(,)?) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Serialize, TS)]
        #[serde(untagged)]
        #[ts(export, untagged)]
        pub enum ActivityEntry {
            $($variant($entry)),+
        }

        impl<'de> Deserialize<'de> for ActivityEntry {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let value = Value::deserialize(deserializer)?;
                let source = value
                    .get("source")
                    .ok_or_else(|| D::Error::missing_field("source"))?;
                let source = Source::deserialize(source).map_err(D::Error::custom)?;
                let entry = match source {
                    $(Source::$source => serde_json::from_value(value).map(Self::$variant)),+
                };
                entry.map_err(|err| D::Error::custom(format!("{source} entry: {err}")))
            }
        }

        impl ActivityEntry {
            pub fn source(&self) -> Source {
                match self {
                    $(Self::$variant(_) => Source::$source),+
                }
            }

            /// The entry's connection, once the runtime stamped it.
            pub fn connection(&self) -> Option<&str> {
                match self {
                    $(Self::$variant(entry) => entry.connection.as_str()),+
                }
            }

            pub fn set_connection(&mut self, name: &ConnectionStamp) {
                let connection = match self {
                    $(Self::$variant(entry) => &mut entry.connection),+
                };
                connection.stamp(name);
            }

            pub fn id(&self) -> &str {
                match self {
                    $(Self::$variant(entry) => &entry.id),+
                }
            }

            pub fn id_mut(&mut self) -> &mut String {
                match self {
                    $(Self::$variant(entry) => &mut entry.id),+
                }
            }

            pub fn timestamp(&self) -> &Timestamp {
                match self {
                    $(Self::$variant(entry) => &entry.timestamp),+
                }
            }
        }

        $(
            impl IntoActivityEntry for $entry {
                fn into_activity_entry(self) -> ActivityEntry {
                    ActivityEntry::$variant(self)
                }
            }
        )+
    };
}

activity_entry!(
    /// An entry of any kind, sent as its kind's object; its `source` says which. Read by that
    /// `source`: an object is read as the kind it names, with that kind's fields, or refused.
    Git(GitEntry) => Git,
    GitLab(GitLabEntry) => Gitlab,
    GitHub(GitHubEntry) => Github,
    Jira(JiraEntry) => Jira,
    Outlook(OutlookEntry) => Outlook,
    System(SystemEntry) => System,
    Calendar(CalendarEntry) => Calendar,
);

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
    /// The meeting part of a meeting entry.
    pub fn meeting(&self) -> Option<&MeetingBody> {
        match self {
            ActivityEntry::Outlook(entry) => Some(&entry.meeting),
            ActivityEntry::Calendar(entry) => Some(&entry.meeting),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests;
