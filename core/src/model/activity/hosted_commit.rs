use chrono::{DateTime, FixedOffset};

use crate::model::{ActivityEntry, ConnectionId, GitEntry, GitLabEntry, Source};

enum HostEvent<'a> {
    GitLab(&'a GitLabEntry),
}

/// An event that names a Git commit, with the identity of the connection that supplied it.
pub(crate) struct HostedCommit<'a> {
    sha: &'a str,
    connection: ConnectionId,
    timestamp: DateTime<FixedOffset>,
    id: &'a str,
    event: HostEvent<'a>,
}

impl<'a> HostedCommit<'a> {
    /// Keep hosted events after local Git when rendering equal instants.
    pub(crate) fn is_host_event(entry: &ActivityEntry) -> bool {
        matches!(entry, ActivityEntry::GitLab(_))
    }

    pub(crate) fn of(entry: &'a ActivityEntry) -> Option<Self> {
        let (sha, event) = match entry {
            ActivityEntry::GitLab(push) if push.entry_type.is_push() => {
                (push.pushed_commit.as_deref()?, HostEvent::GitLab(push))
            }
            _ => return None,
        };
        Some(Self {
            sha,
            connection: ConnectionId::new(entry.source(), entry.connection()?),
            timestamp: entry.timestamp().instant(),
            id: entry.id(),
            event,
        })
    }

    pub(crate) fn sha(&self) -> &'a str {
        self.sha
    }

    pub(crate) fn source(&self) -> Source {
        self.connection.source.clone()
    }

    pub(crate) fn order_key(&self) -> (DateTime<FixedOffset>, &ConnectionId, &str) {
        (self.timestamp, &self.connection, self.id)
    }

    pub(crate) fn attach_to(self, git: &mut GitEntry) {
        match self.event {
            HostEvent::GitLab(entry) => git.gitlab_event = Some(Box::new(entry.clone())),
        }
    }
}
