use std::{
    collections::{BTreeSet, HashSet},
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Result, anyhow};
use chrono::{DateTime, FixedOffset, Utc};
use gix::{
    ObjectId, Repository,
    bstr::{BStr, ByteSlice},
    open::Options as OpenOptions,
    revision::walk::Sorting,
};

use crate::{
    diagnostics,
    model::{CollectOptions, GitEntry, SupportedOs},
    platform::background_command,
    provider::git::{GitConfig, default_git_root},
};

#[cfg(test)]
mod tests;
#[cfg(any(target_os = "windows", test))]
pub(crate) mod windows;

#[cfg(target_os = "windows")]
use self::windows as windows_impl;
#[cfg(not(target_os = "windows"))]
mod windows_impl {
    use anyhow::Result;

    use super::RepoCommits;
    use crate::{model::CollectOptions, provider::git::GitConfig};

    /// WSL needs Windows; elsewhere only the local repositories exist.
    pub(super) fn collect_repos(
        options: &CollectOptions,
        config: &GitConfig,
    ) -> Result<RepoCommits> {
        super::collect_local_repos(options, config)
    }
}

/// Commits found so far, and the repositories skipped for having no `user.email`.
#[derive(Debug, Default)]
pub(crate) struct RepoCommits {
    pub entries: Vec<GitEntry>,
    pub without_email: Vec<String>,
}

// Directories that never hold a user's own repositories but are expensive to walk.
const SKIPPED_DIR_NAMES: &[&str] = &["node_modules", "AppData", ".direnv"];

pub async fn collect_git(
    options: &CollectOptions,
    config: &GitConfig,
    current_os: SupportedOs,
) -> Result<Vec<GitEntry>> {
    let options = *options;
    let config = config.clone();
    let commits = tokio::task::spawn_blocking(move || match current_os {
        SupportedOs::Windows => windows_impl::collect_repos(&options, &config),
        SupportedOs::Macos | SupportedOs::Linux => collect_local_repos(&options, &config),
    })
    .await
    .map_err(|err| anyhow!("Git collection task failed: {err}"))??;
    Ok(finish(commits))
}

/// The basic implementation: repositories on this machine's file system, read with gix.
pub(crate) fn collect_local_repos(
    options: &CollectOptions,
    config: &GitConfig,
) -> Result<RepoCommits> {
    let dirs = resolve_git_repos(config)?;
    let mut entries = Vec::new();
    let mut without_email = Vec::new();
    let mut via_cli = 0;
    diagnostics::info("git", format!("Scanning {} repositories", dirs.len()));

    for repo_path in dirs {
        let repo_name = repo_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string();
        let repo = match open_repo_strict(&repo_path) {
            Ok(repo) => repo,
            Err(err) => {
                diagnostics::warn(
                    "git",
                    format!("Skipped {}: cannot open: {err}", repo_path.display()),
                );
                continue;
            }
        };
        let Some(author_email) = repo_user_email(&repo).or_else(|| {
            let email = git_cli_user_email(&repo_path)?;
            via_cli += 1;
            Some(email)
        }) else {
            without_email.push(repo_name);
            continue;
        };
        match collect_repo_entries(&repo, &repo_name, &author_email, options) {
            Ok(mut repo_entries) => entries.append(&mut repo_entries),
            Err(err) => diagnostics::warn(
                "git",
                format!(
                    "Skipped {}: reading history failed: {err}",
                    repo_path.display()
                ),
            ),
        }
    }

    if via_cli > 0 {
        diagnostics::info(
            "git",
            format!("Read user.email from the git command for {via_cli} repositories"),
        );
    }
    Ok(RepoCommits {
        entries,
        without_email,
    })
}

/// Reports skipped repositories and orders the commits newest first.
fn finish(commits: RepoCommits) -> Vec<GitEntry> {
    let RepoCommits {
        mut entries,
        without_email,
    } = commits;
    if !without_email.is_empty() {
        diagnostics::warn(
            "git",
            format!(
                "Skipped {} repositories without user.email (set it with `git config --global user.email`): {}",
                without_email.len(),
                without_email.join(", ")
            ),
        );
    }

    entries.sort_by(|a, b| {
        match (
            DateTime::parse_from_rfc3339(&a.timestamp).ok(),
            DateTime::parse_from_rfc3339(&b.timestamp).ok(),
        ) {
            (Some(a_time), Some(b_time)) => b_time
                .cmp(&a_time)
                .then_with(|| a.repo.cmp(&b.repo))
                .then_with(|| a.hash.cmp(&b.hash)),
            _ => b
                .timestamp
                .cmp(&a.timestamp)
                .then_with(|| a.repo.cmp(&b.repo))
                .then_with(|| a.hash.cmp(&b.hash)),
        }
    });

    entries
}

pub(crate) fn open_repo_strict(path: &Path) -> Result<Repository> {
    gix::open_opts(path, OpenOptions::default().bail_if_untrusted(true)).map_err(Into::into)
}

pub(crate) fn repo_user_email(repo: &Repository) -> Option<String> {
    let value = repo.config_snapshot().string("user.email")?;
    let email = bstr_to_string(value.as_ref()).trim().to_string();
    (!email.is_empty()).then_some(email)
}

/// Asks the git command when the config files gix reads have no user.email. Git for Windows
/// can find a global config gix does not, for example under `HOMEDRIVE`/`HOMEPATH` on a network
/// home or in the installation's system config.
fn git_cli_user_email(repo_path: &Path) -> Option<String> {
    let output = background_command("git")
        .arg("-C")
        .arg(repo_path)
        .args(["config", "--get", "user.email"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let email = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!email.is_empty()).then_some(email)
}

/// Local repositories: the configured folders (an empty list means none), or else the home
/// folder.
pub(crate) fn resolve_git_repos(config: &GitConfig) -> Result<Vec<PathBuf>> {
    resolve_git_repos_with_home_root(config, default_git_root())
}

pub(crate) fn resolve_git_repos_with_home_root(
    config: &GitConfig,
    home_root: Option<PathBuf>,
) -> Result<Vec<PathBuf>> {
    match &config.dirs {
        Some(dirs) => Ok(resolve_git_repos_from_roots(dirs)),
        None => {
            let home_root = home_root.ok_or_else(|| anyhow!("Git home directory unavailable"))?;
            Ok(expand_git_dir_input(&home_root))
        }
    }
}

pub(crate) fn resolve_git_repos_from_roots(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut repos = BTreeSet::new();

    for root in roots {
        repos.extend(expand_git_dir_input(root));
    }

    repos.into_iter().collect()
}

fn collect_repo_entries(
    repo: &Repository,
    repo_name: &str,
    author_email: &str,
    options: &CollectOptions,
) -> Result<Vec<GitEntry>> {
    let head = head_target(repo);
    let tips = reachable_ref_tips(repo)?;
    let mut entries = Vec::new();

    // Commit time is never before author time in practice, so stopping at the
    // range start skips the rest of history without dropping matches.
    let walk = repo
        .rev_walk(tips)
        .sorting(Sorting::ByCommitTimeCutoff {
            order: Default::default(),
            seconds: options.start.timestamp(),
        })
        .all()?;

    for item in walk {
        let item = item?;
        let commit = item.id().object()?.into_commit();
        let decoded = commit.decode()?;
        let author_signature = commit.author()?;
        let author = bstr_to_string(author_signature.email);
        if author != author_email {
            continue;
        }

        let timestamp = gix_time_to_rfc3339(author_signature.time()?)?;
        let timestamp_utc = DateTime::parse_from_rfc3339(&timestamp)?.with_timezone(&Utc);
        if !options.contains(timestamp_utc) {
            continue;
        }

        let branch = head
            .as_ref()
            .filter(|(head_id, _)| *head_id == commit.id)
            .map(|(_, branch)| branch.clone())
            .unwrap_or_default();

        entries.push(GitEntry::new(
            repo_name.to_string(),
            commit.id.to_string(),
            bstr_to_string(decoded.message_summary().as_ref()),
            author,
            branch,
            timestamp,
        ));
    }

    Ok(entries)
}

fn reachable_ref_tips(repo: &Repository) -> Result<Vec<ObjectId>> {
    let references = repo.references()?;
    let refs = references.all()?;
    let mut seen = HashSet::new();
    let mut tips = Vec::new();

    for reference in refs.peeled()? {
        let reference = reference.map_err(|err| anyhow!(err.to_string()))?;
        let Some(peeled) = reference.detach().peeled else {
            continue;
        };
        if repo.find_commit(peeled).is_err() {
            continue;
        }
        if seen.insert(peeled) {
            tips.push(peeled);
        }
    }

    Ok(tips)
}

fn head_target(repo: &Repository) -> Option<(ObjectId, String)> {
    let branch = repo
        .head_name()
        .ok()
        .flatten()
        .map(|name| name.shorten().to_string())?;
    let commit = repo.head_commit().ok()?;
    Some((commit.id, branch))
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
        if !metadata.is_dir() || is_skipped_dir(&path) {
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

fn is_skipped_dir(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| SKIPPED_DIR_NAMES.contains(&name))
}

fn gix_time_to_rfc3339(time: gix::date::Time) -> Result<String> {
    let datetime = DateTime::from_timestamp(time.seconds, 0)
        .ok_or_else(|| anyhow!("invalid git timestamp: {}", time.seconds))?;
    let offset = FixedOffset::east_opt(time.offset)
        .ok_or_else(|| anyhow!("invalid git timestamp offset: {}", time.offset))?;
    Ok(datetime.with_timezone(&offset).to_rfc3339())
}

fn bstr_to_string(input: &BStr) -> String {
    input.to_str_lossy().into_owned()
}
