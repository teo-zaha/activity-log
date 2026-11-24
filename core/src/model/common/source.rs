use serde::{Deserialize, Serialize};
use strum::{AsRefStr, Display, EnumIter, EnumString, IntoStaticStr};
use ts_rs::TS;

#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    TS,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Display,
    AsRefStr,
    EnumString,
    EnumIter,
    IntoStaticStr,
)]
#[serde(rename_all = "lowercase")]
#[ts(export, rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum Source {
    Git,
    Gitlab,
    Github,
    Jira,
    Outlook,
    System,
    Calendar,
}

impl Source {
    /// The provider's name as people write it.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Git => "Git",
            Self::Gitlab => "GitLab",
            Self::Github => "GitHub",
            Self::Jira => "Jira",
            Self::Outlook => "Outlook",
            Self::System => "System",
            Self::Calendar => "Calendar",
        }
    }
}
