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
use crate::provider::git::GitConfig;
#[cfg(target_os = "windows")]
use crate::{
    diagnostics,
    model::{CollectOptions, GitEntry},
    platform::run_wsl_script,
    provider::git::windows::default_wsl_distros,
};

#[cfg(test)]
mod tests;

const RECORD: char = '\u{1e}';
const FIELD: char = '\u{1f}';

/// What to read from one distribution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WslTarget {
    pub distro: String,
    /// Search `$HOME` for repositories, besides the explicit ones.
    pub discover: bool,
    pub repos: Vec<String>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WslOutput {
    Repos(Vec<WslRepoReport>),
    /// The script could not run git at all, for example because it is not installed.
    Failed(String),
}

/// Distributions to read: those configured (or, unset, every user distribution) searched under
/// `$HOME`, plus the distributions of explicit repositories.
#[cfg(target_os = "windows")]
fn wsl_targets(config: &GitConfig) -> Vec<WslTarget> {
    wsl_targets_with(config, default_wsl_distros)
}

pub(crate) fn wsl_targets_with(
    config: &GitConfig,
    default_distros: impl FnOnce() -> Vec<String>,
) -> Vec<WslTarget> {
    let mut targets = BTreeMap::<String, WslTarget>::new();
    let discovered = config.wsl.distros.clone().unwrap_or_else(default_distros);
    for distro in discovered {
        targets
            .entry(distro.clone())
            .or_insert_with(|| WslTarget {
                distro,
                discover: false,
                repos: Vec::new(),
            })
            .discover = true;
    }
    for repo in &config.wsl.repos {
        targets
            .entry(repo.distro.clone())
            .or_insert_with(|| WslTarget {
                distro: repo.distro.clone(),
                discover: false,
                repos: Vec::new(),
            })
            .repos
            .push(repo.path.clone());
    }
    targets.into_values().collect()
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

/// Adds the WSL commits in `options`; repositories without `user.email` are recorded as
/// `distro:name`. A distribution that fails is logged and skipped.
#[cfg(target_os = "windows")]
fn collect_wsl(config: &GitConfig, options: &CollectOptions, commits: &mut RepoCommits) {
    for target in wsl_targets(config) {
        let repos = match run(&target, Some(options.start)) {
            Ok(repos) => repos,
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
                repos.len(),
                target.distro
            ),
        );
        for repo in repos {
            let name = repo_name(&repo.path).to_string();
            let Some(email) = repo.email else {
                commits
                    .without_email
                    .push(format!("{}:{name}", target.distro));
                continue;
            };
            commits.entries.extend(
                repo.commits
                    .into_iter()
                    .filter(|commit| commit.author_email == email)
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
                    }),
            );
        }
    }
}

/// How many WSL repositories exist, stopping at the first distribution that has any.
#[cfg(target_os = "windows")]
pub(crate) fn count_wsl_repos(config: &GitConfig) -> Result<usize, String> {
    let mut last_error = None;
    for target in wsl_targets(config) {
        match run(&target, None) {
            Ok(repos) if !repos.is_empty() => return Ok(repos.len()),
            Ok(_) => {}
            Err(reason) => last_error = Some(format!("{}: {reason}", target.distro)),
        }
    }
    last_error.map_or(Ok(0), Err)
}

#[cfg(target_os = "windows")]
fn run(target: &WslTarget, since: Option<DateTime<Utc>>) -> Result<Vec<WslRepoReport>, String> {
    let stdout = run_wsl_script(&target.distro, &build_script(target, since))
        .map_err(|err| format!("{err:#}"))?;
    match parse_output(&stdout) {
        WslOutput::Repos(repos) => Ok(repos),
        WslOutput::Failed(reason) => Err(reason),
    }
}

/// The script for one distribution. With `since`, it prints each repository's `user.email`
/// and the commits since then; without, it only lists the repositories.
pub(crate) fn build_script(target: &WslTarget, since: Option<DateTime<Utc>>) -> String {
    let explicit = if target.repos.is_empty() {
        String::new()
    } else {
        let quoted = target
            .repos
            .iter()
            .map(|repo| sh_quote(repo))
            .collect::<Vec<_>>()
            .join(" ");
        format!("  printf '%s\\n' {quoted}\n")
    };
    let discover = if target.discover {
        "  find \"$HOME\" -maxdepth 4 -name .git -prune -print 2>/dev/null | sed 's|/\\.git$||'\n"
    } else {
        ""
    };
    let since = since.map(|since| since.to_rfc3339()).unwrap_or_default();

    format!(
        r#"command -v git >/dev/null 2>&1 || {{ printf '\036E\037git is not installed in this distribution\n'; exit 0; }}
since={since}
repos() {{
  :
{explicit}{discover}}}
repos | sort -u | while IFS= read -r repo; do
  [ -n "$repo" ] && [ -e "$repo/.git" ] || continue
  if [ -z "$since" ]; then
    printf '\036R\037%s\037\n' "$repo"
    continue
  fi
  email=$(git -C "$repo" config --get user.email 2>/dev/null)
  printf '\036R\037%s\037%s\n' "$repo" "$email"
  [ -n "$email" ] || continue
  git -C "$repo" log --all --since="$since" --format='%x1eC%x1f%H%x1f%aI%x1f%ae%x1f%s%x1f%D' 2>/dev/null
done
"#,
        since = sh_quote(&since),
    )
}

/// Reads the script's output. Lines that do not start with the record marker, such as
/// messages from login scripts, are ignored.
pub(crate) fn parse_output(stdout: &str) -> WslOutput {
    let mut repos: Vec<WslRepoReport> = Vec::new();
    for line in stdout.lines() {
        let Some(record) = line.strip_prefix(RECORD) else {
            continue;
        };
        let mut fields = record.split(FIELD);
        match fields.next() {
            Some("E") => return WslOutput::Failed(fields.next().unwrap_or_default().to_string()),
            Some("R") => {
                let path = fields.next().unwrap_or_default().to_string();
                let email = fields
                    .next()
                    .map(str::trim)
                    .filter(|email| !email.is_empty())
                    .map(str::to_string);
                repos.push(WslRepoReport {
                    path,
                    email,
                    commits: Vec::new(),
                });
            }
            Some("C") => {
                let (Some(repo), [Some(hash), Some(time), Some(email), Some(subject)]) = (
                    repos.last_mut(),
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
    WslOutput::Repos(repos)
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
