use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Result, anyhow};
use tokio::process::Command;

use crate::{
    model::{CollectOptions, GitEntry},
    provider::git::{GitConfig, default_git_root},
};

pub async fn collect_git(options: &CollectOptions, config: &GitConfig) -> Result<Vec<GitEntry>> {
    let since = options.since.format("%Y-%m-%d").to_string();
    let until = options.until.format("%Y-%m-%d").to_string();

    let dirs = resolve_git_repos(config)?;

    let mut entries = Vec::new();
    for repo_path in dirs {
        let repo_name = repo_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string();

        let email_output = Command::new("git")
            .arg("config")
            .arg("user.email")
            .current_dir(&repo_path)
            .output()
            .await;
        let email_output = match email_output {
            Ok(output) if output.status.success() => output,
            _ => continue,
        };
        let author_email = String::from_utf8_lossy(&email_output.stdout)
            .trim()
            .to_string();
        if author_email.is_empty() {
            continue;
        }

        let log_output = Command::new("git")
            .arg("log")
            .arg("--all")
            .arg(format!("--after={since}"))
            .arg(format!("--before={until}T23:59:59"))
            .arg(format!("--author={author_email}"))
            .arg("--format=%H|%aI|%ae|%s|%D")
            .current_dir(&repo_path)
            .output()
            .await;
        let log_output = match log_output {
            Ok(output) if output.status.success() => output,
            _ => continue,
        };

        for line in String::from_utf8_lossy(&log_output.stdout).lines() {
            if line.is_empty() {
                continue;
            }
            let mut parts = line.splitn(5, '|');
            let hash = parts.next().unwrap_or_default().to_string();
            let timestamp = parts.next().unwrap_or_default().to_string();
            let author = parts.next().unwrap_or_default().to_string();
            let message = parts.next().unwrap_or_default().to_string();
            let refs = parts.next().unwrap_or_default();
            let branch = refs
                .split(',')
                .map(str::trim)
                .find_map(|value| value.strip_prefix("HEAD -> "))
                .unwrap_or_default()
                .to_string();

            entries.push(GitEntry::new(
                repo_name.clone(),
                hash,
                message,
                author,
                branch,
                timestamp,
            ));
        }
    }

    Ok(entries)
}

pub(crate) async fn git_is_available() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .await
        .map(|output| output.status.success())
        .unwrap_or(false)
}

pub(crate) fn resolve_git_repos(config: &GitConfig) -> Result<Vec<PathBuf>> {
    resolve_git_repos_with_home_root(config, default_git_root())
}

pub(crate) fn resolve_git_repos_with_home_root(
    config: &GitConfig,
    home_root: Option<PathBuf>,
) -> Result<Vec<PathBuf>> {
    match &config.dirs {
        Some(dirs) if !dirs.is_empty() => Ok(resolve_git_repos_from_roots(dirs)),
        _ => discover_home_repos_with_root(home_root),
    }
}

pub(crate) fn resolve_git_repos_from_roots(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut repos = BTreeSet::new();

    for root in roots {
        repos.extend(expand_git_dir_input(root));
    }

    repos.into_iter().collect()
}

fn discover_home_repos_with_root(home_root: Option<PathBuf>) -> Result<Vec<PathBuf>> {
    let home_root = home_root.ok_or_else(|| anyhow!("Git home directory unavailable"))?;
    Ok(expand_git_dir_input(&home_root))
}

fn expand_git_dir_input(path: &Path) -> Vec<PathBuf> {
    if path.join(".git").exists() {
        return vec![path.to_path_buf()];
    }

    discover_repos(path, 0, 4)
}

fn discover_repos(root: &Path, depth: usize, max_depth: usize) -> Vec<PathBuf> {
    if depth >= max_depth {
        return Vec::new();
    }

    let mut repos = Vec::new();
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(_) => return repos,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        if !metadata.is_dir() {
            continue;
        }

        if path.join(".git").exists() {
            repos.push(path);
        } else {
            repos.extend(discover_repos(&path, depth + 1, max_depth));
        }
    }

    repos
}
