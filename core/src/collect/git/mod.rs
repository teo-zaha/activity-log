use std::{
    collections::{BTreeSet, HashSet},
    fs,
    ops::ControlFlow,
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use anyhow::{Result, anyhow, bail};
use chrono::{DateTime, FixedOffset, Utc};
use gix::{
    ObjectId, Repository,
    bstr::{BStr, ByteSlice},
    open::Options as OpenOptions,
    revision::walk::Sorting,
};

use crate::{
    diagnostics,
    model::{CollectOptions, GitEntry, Timestamp},
    platform::{
        blocking::{Stopped, blocking_until_dropped},
        parsed_output_command,
    },
    provider::{
        connection::ConnectionLocation,
        git::{GitConfig, default_git_root},
    },
};

#[cfg(test)]
mod tests;
#[cfg(any(target_os = "windows", test))]
pub(crate) mod windows;

#[cfg(target_os = "windows")]
use self::windows as windows_impl;
#[cfg(not(target_os = "windows"))]
mod windows_impl {
    use anyhow::{Result, bail};

    use super::{RepoCommits, WslTarget};
    use crate::model::CollectOptions;

    pub(crate) fn collect_wsl(
        target: &WslTarget,
        _options: &CollectOptions,
        _cancelled: &std::sync::atomic::AtomicBool,
    ) -> Result<RepoCommits> {
        bail!("WSL {} needs Windows", target.distro)
    }
}

/// What to read from one WSL distribution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WslTarget {
    pub distro: String,
    /// Repositories, or folders searched for them; `~` is the distribution's home.
    pub dirs: Vec<String>,
}

/// Everything read from the repositories, before keeping the user's own commits.
#[derive(Debug, Default)]
pub(crate) struct RepoCommits {
    pub repos: Vec<RepoHistory>,
    /// Emails configured outside any repository, such as a WSL distribution's global config.
    pub identities: Vec<String>,
}

/// One repository's commits in the range, by every author.
#[derive(Debug)]
pub(crate) struct RepoHistory {
    /// The name used in messages, such as `api` or `NixOS:api`.
    pub label: String,
    /// The repository's effective `user.email`.
    pub email: Option<String>,
    pub commits: Vec<GitEntry>,
}

/// Emails found while collecting, across every Git connection, so a repository without
/// `user.email` is matched by the emails of the others, even in another location.
#[derive(Debug, Default)]
pub struct GitCollection {
    found: Mutex<FoundIdentities>,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct FoundIdentities {
    /// Lowercased: git hosts compare emails case-insensitively.
    pub identities: BTreeSet<String>,
    pub without_email: Vec<String>,
}

impl GitCollection {
    pub(crate) fn record(&self, commits: &RepoCommits) {
        let mut found = self
            .found
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        found.identities.extend(
            commits
                .repos
                .iter()
                .filter_map(|repo| repo.email.as_deref())
                .chain(commits.identities.iter().map(String::as_str))
                .map(normalize_email)
                .filter(|email| !email.is_empty()),
        );
        found.without_email.extend(
            commits
                .repos
                .iter()
                .filter(|repo| repo.email.is_none())
                .map(|repo| repo.label.clone()),
        );
    }

    pub(crate) fn found(&self) -> FoundIdentities {
        self.found
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
}

/// Keeps the commits authored with one of `identities`; returns them and how many were not.
pub(crate) fn keep_own(
    identities: &BTreeSet<String>,
    commits: Vec<GitEntry>,
) -> (Vec<GitEntry>, usize) {
    let (own, other): (Vec<_>, Vec<_>) = commits
        .into_iter()
        .partition(|commit| identities.contains(&normalize_email(&commit.author)));
    (own, other.len())
}

/// Logs how commits were matched across all Git connections.
pub(crate) fn report_identities(found: &FoundIdentities, ignored: usize) {
    if found.identities.is_empty() {
        if !found.without_email.is_empty() {
            diagnostics::warn(
                "git",
                format!(
                    "Skipped {} repositories: no user.email is set in any of them, so your commits cannot be told apart (set it with `git config --global user.email`): {}",
                    found.without_email.len(),
                    found.without_email.join(", ")
                ),
            );
        }
        return;
    }
    // Repositories without their own user.email are matched by these too.
    let ignored = if ignored > 0 {
        format!("; ignored {ignored} by other authors")
    } else {
        String::new()
    };
    let identities = found.identities.iter().cloned().collect::<Vec<_>>();
    diagnostics::info(
        "git",
        format!("Keeping commits by {}{ignored}", identities.join(", ")),
    );
}

/// One collection's own commits, as the provider gets them across connections.
#[cfg(test)]
pub(crate) fn own_commits(commits: RepoCommits) -> (FoundIdentities, Vec<GitEntry>, usize) {
    let collection = GitCollection::default();
    collection.record(&commits);
    let found = collection.found();
    let all = commits
        .repos
        .into_iter()
        .flat_map(|repo| repo.commits)
        .collect();
    let (own, ignored) = keep_own(&found.identities, all);
    (found, own, ignored)
}

// Directories that never hold a user's own repositories but are expensive to walk.
const SKIPPED_DIR_NAMES: &[&str] = &["node_modules", "AppData", ".direnv"];

/// Reads the repositories of one Git connection, with commits by every author; the provider
/// keeps the user's own once all connections are read.
pub(crate) async fn collect_git(
    options: &CollectOptions,
    config: &GitConfig,
    location: ConnectionLocation,
) -> Result<RepoCommits> {
    let options = *options;
    let config = config.clone();
    blocking_until_dropped("Git collection", move |cancelled| match location {
        ConnectionLocation::Local => collect_local_repos(&options, &config, cancelled),
        ConnectionLocation::Wsl(distro) => {
            let target = WslTarget {
                distro,
                dirs: config.wsl_dirs()?,
            };
            windows_impl::collect_wsl(&target, &options, cancelled)
        }
    })
    .await?
}

/// The basic implementation: repositories on this machine's file system, read with gix. Stops
/// between repositories and commits once `cancelled` is set.
pub(crate) fn collect_local_repos(
    options: &CollectOptions,
    config: &GitConfig,
    cancelled: &AtomicBool,
) -> Result<RepoCommits> {
    let stopped = || anyhow::Error::new(Stopped);
    let dirs = resolve_git_repos(config, cancelled)?;
    if cancelled.load(Ordering::Relaxed) {
        return Err(stopped());
    }
    let mut repos = Vec::new();
    let mut via_cli = 0;
    let mut seen = HashSet::new();
    let mut worktrees = 0;
    diagnostics::info("git", format!("Scanning {} repositories", dirs.len()));

    for repo_path in dirs {
        if cancelled.load(Ordering::Relaxed) {
            return Err(stopped());
        }
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
        // Worktrees of one repository share its history; it is read once, under its own name.
        let common_dir = repo.common_dir();
        let common_dir = fs::canonicalize(common_dir).unwrap_or_else(|_| common_dir.to_path_buf());
        if !seen.insert(common_dir.clone()) {
            worktrees += 1;
            continue;
        }
        let repo_name = repo_label(&common_dir);
        let email = repo_user_email(&repo).or_else(|| {
            let email = git_cli_user_email(&repo_path)?;
            via_cli += 1;
            Some(email)
        });
        let commits = match collect_repo_entries(&repo, &repo_name, options, cancelled) {
            Ok(commits) => commits,
            Err(_) if cancelled.load(Ordering::Relaxed) => return Err(stopped()),
            Err(err) => {
                diagnostics::warn(
                    "git",
                    format!(
                        "Skipped {}: reading history failed: {err}",
                        repo_path.display()
                    ),
                );
                Vec::new()
            }
        };
        repos.push(RepoHistory {
            label: repo_name,
            email,
            commits,
        });
    }

    if via_cli > 0 {
        diagnostics::info(
            "git",
            format!("Read user.email from the git command for {via_cli} repositories"),
        );
    }
    if worktrees > 0 {
        diagnostics::info(
            "git",
            format!("Read {worktrees} more worktrees through their main repository"),
        );
    }
    Ok(RepoCommits {
        repos,
        identities: Vec::new(),
    })
}

/// A repository's name: the folder of its main worktree (`…/activity-log/.git` →
/// `activity-log`), or of a bare repository without the `.git` suffix.
pub(crate) fn repo_label(common_dir: &Path) -> String {
    let dir = if common_dir.file_name().is_some_and(|name| name == ".git") {
        common_dir.parent().unwrap_or(common_dir)
    } else {
        common_dir
    };
    let name = dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    name.strip_suffix(".git").unwrap_or(name).to_string()
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
    let output = parsed_output_command("git")
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
/// folder, found until `cancelled` is set.
pub(crate) fn resolve_git_repos(
    config: &GitConfig,
    cancelled: &AtomicBool,
) -> Result<Vec<PathBuf>> {
    Ok(resolve_git_repos_from_roots_until(
        &git_roots(config, default_git_root())?,
        cancelled,
    ))
}

#[cfg(test)]
pub(crate) fn resolve_git_repos_with_home_root(
    config: &GitConfig,
    home_root: Option<PathBuf>,
) -> Result<Vec<PathBuf>> {
    Ok(resolve_git_repos_from_roots(&git_roots(config, home_root)?))
}

#[cfg(test)]
pub(crate) fn resolve_git_repos_from_roots(roots: &[PathBuf]) -> Vec<PathBuf> {
    resolve_git_repos_from_roots_until(roots, &AtomicBool::new(false))
}

/// The repositories under `roots`, found until `cancelled` is set.
fn resolve_git_repos_from_roots_until(roots: &[PathBuf], cancelled: &AtomicBool) -> Vec<PathBuf> {
    let mut repos = BTreeSet::new();
    for root in roots {
        let found = visit_git_dir_input(root, cancelled, &mut |repo| {
            repos.insert(repo);
            ControlFlow::Continue(())
        });
        if found.is_break() {
            break;
        }
    }
    repos.into_iter().collect()
}

/// The folders a connection searches: its configured ones, or the home folder.
fn git_roots(config: &GitConfig, home_root: Option<PathBuf>) -> Result<Vec<PathBuf>> {
    match &config.dirs {
        Some(dirs) => Ok(dirs.clone()),
        None => home_root
            .map(|home| vec![home])
            .ok_or_else(|| anyhow!("Git home directory unavailable")),
    }
}

/// The first folder the connection searches that the OS refuses to list: where nothing found
/// means no permission, not no repositories.
pub(crate) fn denied_root(config: &GitConfig) -> Option<PathBuf> {
    git_roots(config, default_git_root())
        .ok()?
        .into_iter()
        .find(|root| {
            fs::read_dir(root).is_err_and(|err| err.kind() == std::io::ErrorKind::PermissionDenied)
        })
}

/// Calls `visit` with each repository the connection would read, until it breaks.
pub(crate) fn visit_git_repos(
    config: &GitConfig,
    visit: &mut impl FnMut(PathBuf) -> ControlFlow<()>,
) -> Result<()> {
    for root in git_roots(config, default_git_root())? {
        if visit_git_dir_input(&root, &AtomicBool::new(false), visit).is_break() {
            break;
        }
    }
    Ok(())
}

/// Commits are kept by author time, but history can only be cut short by commit time, and a
/// commit can be recorded before it was authored or before its parent (a clock off, a rebase
/// or patches applied with their dates). So history is read from this long before the range:
/// such work is found within the margin, and a branch tip is always read with its parents,
/// however old its own date. Reading all of history would cost about 5 µs per commit of every
/// repository in each collection (2 s for 400,000 commits).
const HISTORY_MARGIN: chrono::Duration = chrono::Duration::days(30);

/// The commit time before which history is not read, for `options`.
pub(crate) fn history_cutoff(options: &CollectOptions) -> DateTime<Utc> {
    options.start - HISTORY_MARGIN
}

/// Every commit in the range reachable from any ref, by any author.
fn collect_repo_entries(
    repo: &Repository,
    repo_name: &str,
    options: &CollectOptions,
    cancelled: &AtomicBool,
) -> Result<Vec<GitEntry>> {
    let head = head_target(repo);
    let cutoff = history_cutoff(options).timestamp();
    let mut entries = Vec::new();
    let mut keep = |commit: gix::Commit<'_>| -> Result<()> {
        let decoded = commit.decode()?;
        let author_signature = commit.author()?;
        let author = bstr_to_string(author_signature.email);
        let timestamp = Timestamp::parse(gix_time_to_rfc3339(author_signature.time()?)?)?;
        if !options.contains(timestamp.utc()) {
            return Ok(());
        }
        let branch = head
            .as_ref()
            .filter(|(head_id, _)| *head_id == commit.id)
            .map(|(_, branch)| branch.clone());
        entries.push(GitEntry::new(
            repo_name.to_string(),
            commit.id.to_string(),
            bstr_to_string(decoded.message_summary().as_ref()),
            author,
            branch,
            timestamp,
        ));
        Ok(())
    };

    // The walk below leaves out every commit recorded before the cutoff, with what is behind
    // it; a tip that old is read here and the walk starts from its parents.
    let mut starts = Vec::new();
    for tip in reachable_ref_tips(repo)? {
        let commit = repo.find_commit(tip)?;
        if commit.time()?.seconds >= cutoff {
            starts.push(tip);
        } else {
            starts.extend(commit.parent_ids().map(|id| id.detach()));
            keep(commit)?;
        }
    }

    let walk = repo
        .rev_walk(starts)
        .sorting(Sorting::ByCommitTimeCutoff {
            order: Default::default(),
            seconds: cutoff,
        })
        .all()?;
    for item in walk {
        if cancelled.load(Ordering::Relaxed) {
            bail!(Stopped);
        }
        keep(item?.id().object()?.into_commit())?;
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

/// Visits `path` or the repositories under it; a set `cancelled` ends the walk like a break.
fn visit_git_dir_input(
    path: &Path,
    cancelled: &AtomicBool,
    visit: &mut impl FnMut(PathBuf) -> ControlFlow<()>,
) -> ControlFlow<()> {
    if path.join(".git").exists() {
        return visit(path.to_path_buf());
    }
    visit_repos(path, 0, 4, cancelled, visit)
}

fn visit_repos(
    root: &Path,
    depth: usize,
    max_depth: usize,
    cancelled: &AtomicBool,
    visit: &mut impl FnMut(PathBuf) -> ControlFlow<()>,
) -> ControlFlow<()> {
    if depth >= max_depth {
        return ControlFlow::Continue(());
    }
    let Ok(entries) = fs::read_dir(root) else {
        return ControlFlow::Continue(());
    };

    for entry in entries.flatten() {
        if cancelled.load(Ordering::Relaxed) {
            return ControlFlow::Break(());
        }
        let path = entry.path();
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if !metadata.is_dir() || is_skipped_dir(&path) {
            continue;
        }

        if path.join(".git").exists() {
            visit(path)?;
        } else {
            visit_repos(&path, depth + 1, max_depth, cancelled, visit)?;
        }
    }
    ControlFlow::Continue(())
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
