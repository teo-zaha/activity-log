use std::fmt;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub enum GitLabType {
    PushedTo,
    Created,
    Opened,
    Closed,
    Reopened,
    Updated,
    Merged,
    Accepted,
    CommentedOn,
    Approved,
    Joined,
    Destroyed,
    Expired,
    Unknown,
}

impl GitLabType {
    // GitLab emits human-readable action names like "pushed to"; map those once at ingestion.
    pub fn from_gitlab_action_name(value: &str) -> Self {
        match value {
            "pushed to" => Self::PushedTo,
            "created" => Self::Created,
            "opened" => Self::Opened,
            "closed" => Self::Closed,
            "reopened" => Self::Reopened,
            "updated" => Self::Updated,
            "merged" => Self::Merged,
            "accepted" => Self::Accepted,
            "commented on" => Self::CommentedOn,
            "approved" => Self::Approved,
            "joined" => Self::Joined,
            "destroyed" => Self::Destroyed,
            "expired" => Self::Expired,
            _ => Self::Unknown,
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::PushedTo => "pushedTo",
            Self::Created => "created",
            Self::Opened => "opened",
            Self::Closed => "closed",
            Self::Reopened => "reopened",
            Self::Updated => "updated",
            Self::Merged => "merged",
            Self::Accepted => "accepted",
            Self::CommentedOn => "commentedOn",
            Self::Approved => "approved",
            Self::Joined => "joined",
            Self::Destroyed => "destroyed",
            Self::Expired => "expired",
            Self::Unknown => "unknown",
        }
    }
}

impl fmt::Display for GitLabType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
