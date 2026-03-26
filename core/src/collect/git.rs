use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::Result;

use crate::{
    model::{CollectOptions, GitEntry},
    provider::{git::GIT_DIRS, provider_context::ProviderContext},
    utils::ensure_valid_date,
};

pub fn collect_git(options: &CollectOptions, context: &ProviderContext) -> Result<Vec<GitEntry>> {
    ensure_valid_date(&options.since, "since")?;
    ensure_valid_date(&options.until, "until")?;

    let dirs = options
        .git_dirs
        .clone()
        .or_else(|| resolve_git_dirs_from_context(context))
        .unwrap_or_else(discover_home_repos);

    let mut entries = Vec::new();
    for repo_path in dirs {
        let git_dir = repo_path.join(".git");
        if !git_dir.exists() {
            continue;
        }

        let repo_name = repo_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string();

        let email_output = Command::new("git")
            .arg("config")
            .arg("user.email")
            .current_dir(&repo_path)
            .output();
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
            .arg(format!("--after={}", options.since))
            .arg(format!("--before={}T23:59:59", options.until))
            .arg(format!("--author={author_email}"))
            .arg("--format=%H|%aI|%ae|%s|%D")
            .current_dir(&repo_path)
            .output();
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

pub(crate) fn git_is_available() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

pub(crate) fn resolve_git_dirs_from_context(context: &ProviderContext) -> Option<Vec<PathBuf>> {
    context.get(GIT_DIRS).and_then(|value| {
        let dirs = value
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .collect::<Vec<_>>();
        (!dirs.is_empty()).then_some(dirs)
    })
}

pub(crate) fn discover_home_repos() -> Vec<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| discover_repos(&home, 0, 4))
        .unwrap_or_default()
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
