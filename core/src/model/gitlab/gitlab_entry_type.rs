use serde::{Deserialize, Serialize};
use strum::{AsRefStr, Display, EnumString};
use ts_rs::TS;

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, Display, AsRefStr, EnumString,
)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
#[strum(serialize_all = "camelCase", ascii_case_insensitive)]
pub enum GitLabType {
    #[strum(serialize = "pushed to")]
    PushedTo,
    /// A push that created its branch.
    #[strum(serialize = "pushed new")]
    PushedNew,
    Created,
    Opened,
    Closed,
    Reopened,
    Updated,
    Merged,
    Accepted,
    #[strum(serialize = "commented on")]
    CommentedOn,
    Approved,
    Joined,
    Destroyed,
    Expired,
    /// An action this version does not know yet.
    Unknown,
}

impl GitLabType {
    // GitLab emits human-readable action names like "pushed to"; map those once at ingestion.
    pub fn from_gitlab_action_name(value: &str) -> Self {
        value.parse().unwrap_or(Self::Unknown)
    }

    /// A push to a branch, new or not: the only events that name a commit.
    pub fn is_push(&self) -> bool {
        matches!(self, Self::PushedTo | Self::PushedNew)
    }
}
