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
    Unknown,
}

impl GitLabType {
    // GitLab emits human-readable action names like "pushed to"; map those once at ingestion.
    pub fn from_gitlab_action_name(value: &str) -> Self {
        value.parse().unwrap_or(Self::Unknown)
    }
}
