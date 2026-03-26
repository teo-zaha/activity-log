use crate::model::{
    git::git_entry::GitEntry, gitlab::gitlab_entry::GitLabEntry, jira::jira_entry::JiraEntry,
    outlook::outlook_entry::OutlookEntry, system::system_entry::SystemEntry,
};

#[derive(Debug)]
pub enum ProviderCollection {
    Git(Vec<GitEntry>),
    GitLab(Vec<GitLabEntry>),
    Jira(Vec<JiraEntry>),
    Outlook(Vec<OutlookEntry>),
    System(Vec<SystemEntry>),
}
