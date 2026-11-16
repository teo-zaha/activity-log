//! Windows only: repositories inside WSL distributions, read by running `git` in the
//! distribution.
//!
//! Reading them over `\\wsl.localhost` would be slow and trips Git's ownership check, so each
//! distribution gets one `wsl.exe` call running a POSIX shell script that finds the
//! repositories and prints their history in a line format parsed here.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

#[cfg(target_os = "windows")]
use super::RepoCommits;
use super::RepoHistory;
#[cfg(target_os = "windows")]
use crate::{diagnostics, platform::run_wsl_script, provider::git::windows::default_wsl_dirs};
use crate::{
    model::{CollectOptions, GitEntry},
    provider::git::GitConfig,
};

#[cfg(test)]
mod tests;

const RECORD: char = '\u{1e}';
const FIELD: char = '\u{1f}';

/// What to read from one distribution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WslTarget {
    pub distro: String,
    /// Repositories, or folders searched for them; `~` is the distribution's home.
    pub dirs: Vec<String>,
}

/// One repository as the script reported it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WslRepoReport {
    pub path: String,
    pub email: Option<String>,
    pub commits: Vec<WslCommit>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WslCommit {
    pub hash: String,
    pub author_time: String,
    pub author_email: String,
    pub subject: String,
    /// Set for the commit `HEAD` points at, like the local collector.
    pub branch: String,
}

/// What the script found in one distribution.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct WslScan {
    /// The distribution's global `user.email`, an identity even where repositories lack one.
    pub global_email: Option<String>,
    pub repos: Vec<WslRepoReport>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WslOutput {
    Scanned(WslScan),
    /// The script could not run git at all, for example because it is not installed.
    Failed(String),
}

/// Distributions to read with their folders: those configured or, unset, the home of every
/// user distribution. A distribution without folders is not started.
#[cfg(target_os = "windows")]
fn wsl_targets(config: &GitConfig) -> Vec<WslTarget> {
    wsl_targets_with(config, default_wsl_dirs)
}

pub(crate) fn wsl_targets_with(
    config: &GitConfig,
    default_dirs: impl FnOnce() -> BTreeMap<String, Vec<String>>,
) -> Vec<WslTarget> {
    config
        .wsl
        .dirs
        .clone()
        .unwrap_or_else(default_dirs)
        .into_iter()
        .filter(|(_, dirs)| !dirs.is_empty())
        .map(|(distro, dirs)| WslTarget { distro, dirs })
        .collect()
}

/// The local repositories, as everywhere, plus those inside WSL.
#[cfg(target_os = "windows")]
pub(super) fn collect_repos(
    options: &CollectOptions,
    config: &GitConfig,
) -> anyhow::Result<RepoCommits> {
    let mut commits = super::collect_local_repos(options, config)?;
    collect_wsl(config, options, &mut commits);
    Ok(commits)
}

/// Adds the WSL repositories, labelled `distro:name`, with their commits in `options`. A
/// distribution that fails is logged and skipped.
#[cfg(target_os = "windows")]
fn collect_wsl(config: &GitConfig, options: &CollectOptions, commits: &mut RepoCommits) {
    for target in wsl_targets(config) {
        let scan = match run(&target, Some(options.start)) {
            Ok(scan) => scan,
            Err(reason) => {
                diagnostics::warn(
                    "git",
                    format!("Skipped WSL distribution {}: {reason}", target.distro),
                );
                continue;
            }
        };
        diagnostics::info(
            "git",
            format!(
                "Scanning {} repositories in WSL {}",
                scan.repos.len(),
                target.distro
            ),
        );
        commits.identities.extend(scan.global_email);
        commits.repos.extend(
            scan.repos
                .into_iter()
                .map(|repo| repo_history(&target.distro, repo, options)),
        );
    }
}

/// One reported repository, with the commits whose author time is in the range (`--since`
/// selects by commit time).
pub(crate) fn repo_history(
    distro: &str,
    repo: WslRepoReport,
    options: &CollectOptions,
) -> RepoHistory {
    let name = repo_name(&repo.path).to_string();
    let commits = repo
        .commits
        .into_iter()
        .filter(|commit| {
            DateTime::parse_from_rfc3339(&commit.author_time)
                .is_ok_and(|at| options.contains(at.to_utc()))
        })
        .map(|commit| {
            GitEntry::new(
                name.clone(),
                commit.hash,
                commit.subject,
                commit.author_email,
                commit.branch,
                commit.author_time,
            )
        })
        .collect();
    RepoHistory {
        label: format!("{distro}:{name}"),
        email: repo.email,
        commits,
    }
}

/// How many WSL repositories exist, stopping at the first distribution that has any.
#[cfg(target_os = "windows")]
pub(crate) fn count_wsl_repos(config: &GitConfig) -> Result<usize, String> {
    let mut last_error = None;
    for target in wsl_targets(config) {
        match run(&target, None) {
            Ok(scan) if !scan.repos.is_empty() => return Ok(scan.repos.len()),
            Ok(_) => {}
            Err(reason) => last_error = Some(format!("{}: {reason}", target.distro)),
        }
    }
    last_error.map_or(Ok(0), Err)
}

#[cfg(target_os = "windows")]
fn run(target: &WslTarget, since: Option<DateTime<Utc>>) -> Result<WslScan, String> {
    let stdout = run_wsl_script(&target.distro, &build_script(target, since))
        .map_err(|err| format!("{err:#}"))?;
    match parse_output(&stdout) {
        WslOutput::Scanned(scan) => Ok(scan),
        WslOutput::Failed(reason) => Err(reason),
    }
}

/// The script for one distribution. With `since`, it prints the global `user.email`, then each
/// repository's `user.email` and its commits since then by every author (the collector decides
/// which are the user's); without, it only lists the repositories.
pub(crate) fn build_script(target: &WslTarget, since: Option<DateTime<Utc>>) -> String {
    let dirs = target
        .dirs
        .iter()
        .map(|dir| sh_quote(dir))
        .collect::<Vec<_>>()
        .join(" ");
    let since = since.map(|since| since.to_rfc3339()).unwrap_or_default();

    format!(
        r#"command -v git >/dev/null 2>&1 || {{ printf '\036E\037git is not installed in this distribution\n'; exit 0; }}
since={since}
[ -z "$since" ] || printf '\036G\037%s\n' "$(git config --global --get user.email 2>/dev/null)"
repos() {{
  for dir in {dirs}; do
    case $dir in
      "~") dir=$HOME ;;
      "~/"*) dir=$HOME/${{dir#"~/"}} ;;
    esac
    if [ -e "$dir/.git" ]; then
      printf '%s\n' "$dir"
    else
      find "$dir" -maxdepth 4 -name .git -prune -print 2>/dev/null | sed 's|/\.git$||'
    fi
  done
}}
repos | sort -u | while IFS= read -r repo; do
  [ -n "$repo" ] && [ -e "$repo/.git" ] || continue
  if [ -z "$since" ]; then
    printf '\036R\037%s\037\n' "$repo"
    continue
  fi
  email=$(git -C "$repo" config --get user.email 2>/dev/null)
  printf '\036R\037%s\037%s\n' "$repo" "$email"
  git -C "$repo" log --all --since="$since" --format='%x1eC%x1f%H%x1f%aI%x1f%ae%x1f%s%x1f%D' 2>/dev/null
done
"#,
        since = sh_quote(&since),
    )
}

/// Reads the script's output. Lines that do not start with the record marker, such as
/// messages from login scripts, are ignored.
pub(crate) fn parse_output(stdout: &str) -> WslOutput {
    let mut scan = WslScan::default();
    for line in stdout.lines() {
        let Some(record) = line.strip_prefix(RECORD) else {
            continue;
        };
        let mut fields = record.split(FIELD);
        match fields.next() {
            Some("E") => return WslOutput::Failed(fields.next().unwrap_or_default().to_string()),
            Some("G") => scan.global_email = non_empty(fields.next()),
            Some("R") => {
                let path = fields.next().unwrap_or_default().to_string();
                let email = non_empty(fields.next());
                scan.repos.push(WslRepoReport {
                    path,
                    email,
                    commits: Vec::new(),
                });
            }
            Some("C") => {
                let (Some(repo), [Some(hash), Some(time), Some(email), Some(subject)]) = (
                    scan.repos.last_mut(),
                    [fields.next(), fields.next(), fields.next(), fields.next()],
                ) else {
                    continue;
                };
                let refs = fields.next().unwrap_or_default();
                repo.commits.push(WslCommit {
                    hash: hash.to_string(),
                    author_time: time.to_string(),
                    author_email: email.to_string(),
                    subject: subject.to_string(),
                    branch: head_branch(refs).unwrap_or_default().to_string(),
                });
            }
            _ => {}
        }
    }
    WslOutput::Scanned(scan)
}

fn non_empty(field: Option<&str>) -> Option<String> {
    field
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// `main` from `%D` output such as `HEAD -> main, origin/main`.
fn head_branch(refs: &str) -> Option<&str> {
    refs.split(',')
        .map(str::trim)
        .find_map(|name| name.strip_prefix("HEAD -> "))
}

/// The last path segment, used as the repository name like for local ones.
pub(crate) fn repo_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Single-quotes a value for POSIX `sh`.
fn sh_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}
