use crate::model::{git::git_entry::GitEntry, gitlab::gitlab_entry::GitLabEntry};

#[derive(Debug, Clone)]
pub struct MergeResult {
    pub merged_git: Vec<GitEntry>,
    pub remaining_gitlab: Vec<GitLabEntry>,
}
