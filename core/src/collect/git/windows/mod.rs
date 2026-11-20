//! Windows only: repositories inside WSL distributions, read by running `git` in the
//! distribution.
//!
//! Reading them over `\\wsl.localhost` would be slow and trips Git's ownership check, so each
//! distribution gets one `wsl.exe` call running a POSIX shell script that finds the
//! repositories and prints their history in a line format parsed here.

use chrono::{DateTime, Utc};

#[cfg(target_os = "windows")]
use super::RepoCommits;
use super::{RepoHistory, WslTarget};
use crate::model::{CollectOptions, GitEntry};
#[cfg(target_os = "windows")]
use crate::{diagnostics, platform::run_wsl_script};

#[cfg(test)]
mod tests;

const RECORD: char = '\u{1e}';
const FIELD: char = '\u{1f}';

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

/// One distribution's repositories, with their commits in `options`.
#[cfg(target_os = "windows")]
pub(crate) fn collect_wsl(
    target: &WslTarget,
    options: &CollectOptions,
    cancelled: &std::sync::atomic::AtomicBool,
) -> anyhow::Result<RepoCommits> {
    let scan = run(target, Some(super::history_cutoff(options)), cancelled)
        .map_err(|reason| anyhow::anyhow!(reason))?;
    diagnostics::info(
        "git",
        format!(
            "Scanning {} repositories in WSL {}",
            scan.repos.len(),
            target.distro
        ),
    );
    Ok(RepoCommits {
        identities: scan.global_email.into_iter().collect(),
        repos: scan
            .repos
            .into_iter()
            .map(|repo| repo_history(&target.distro, repo, options))
            .collect(),
    })
}

/// One reported repository, with the commits whose author time is in the range (`--since`
/// selects by commit time, from `history_cutoff`).
pub(crate) fn repo_history(
    distro: &str,
    repo: WslRepoReport,
    options: &CollectOptions,
) -> RepoHistory {
    let name = repo_name(&repo.path).to_string();
    // The script prints a commit twice when an old tip's history meets the rest.
    let mut seen = std::collections::HashSet::new();
    let commits = repo
        .commits
        .into_iter()
        .filter(|commit| {
            DateTime::parse_from_rfc3339(&commit.author_time)
                .is_ok_and(|at| options.contains(at.to_utc()))
                && seen.insert(commit.hash.clone())
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

/// Whether the distribution has a repository under the target's folders.
#[cfg(target_os = "windows")]
pub(crate) fn has_wsl_repos(
    target: &WslTarget,
    cancelled: &std::sync::atomic::AtomicBool,
) -> Result<bool, String> {
    run(target, None, cancelled).map(|scan| !scan.repos.is_empty())
}

#[cfg(target_os = "windows")]
fn run(
    target: &WslTarget,
    since: Option<DateTime<Utc>>,
    cancelled: &std::sync::atomic::AtomicBool,
) -> Result<WslScan, String> {
    let stdout = run_wsl_script(&target.distro, &build_script(target, since), cancelled)
        .map_err(|err| format!("{err:#}"))?;
    match parse_output(&stdout) {
        WslOutput::Scanned(scan) => Ok(scan),
        WslOutput::Failed(reason) => Err(reason),
    }
}

/// The script for one distribution. With `since`, it prints the global `user.email`, then each
/// repository's `user.email` and its commits since then by every author (the collector decides
/// which are the user's); without, it only prints the first repository it finds.
pub(crate) fn build_script(target: &WslTarget, since: Option<DateTime<Utc>>) -> String {
    let dirs = target
        .dirs
        .iter()
        .map(|dir| sh_quote(dir))
        .collect::<Vec<_>>()
        .join(" ");
    let since_unix = since
        .map(|since| since.timestamp().to_string())
        .unwrap_or_default();
    let since = since.map(|since| since.to_rfc3339()).unwrap_or_default();

    // The C locale inside the distribution too: `wsl.exe` does not pass this process's
    // environment in, and git's output is parsed.
    format!(
        r#"export LC_ALL=C
command -v git >/dev/null 2>&1 || {{ printf '\036E\037git is not installed in this distribution\n'; exit 0; }}
since={since}
since_unix={since_unix}
format='%x1eC%x1f%H%x1f%aI%x1f%ae%x1f%s%x1f%D'
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
seen=' '
{{ if [ -z "$since" ]; then repos; else repos | sort -u; fi; }} | while IFS= read -r repo; do
  [ -n "$repo" ] && [ -e "$repo/.git" ] || continue
  # Worktrees of one repository share its history: it is read once, under its main folder.
  common=$(git -C "$repo" rev-parse --path-format=absolute --git-common-dir 2>/dev/null) || common="$repo/.git"
  case $seen in *" $common "*) continue ;; esac
  seen="$seen$common "
  main=${{common%/.git}}
  if [ -z "$since" ]; then
    printf '\036R\037%s\037\n' "$main"
    exit 0
  fi
  email=$(git -C "$repo" config --get user.email 2>/dev/null)
  printf '\036R\037%s\037%s\n' "$main" "$email"
  git -C "$repo" log --all --since="$since" --format="$format" 2>/dev/null
  # `--since` leaves out a tip recorded before it, and what is behind it: such a tip is read
  # with its parents' history.
  {{ git -C "$repo" for-each-ref --format='%(objectname) %(committerdate:unix)' refs/heads refs/remotes refs/tags 2>/dev/null; git -C "$repo" log -1 --format='%H %ct' HEAD 2>/dev/null; }} |
  while read -r tip date; do
    [ -n "$date" ] && [ "$date" -lt "$since_unix" ] || continue
    git -C "$repo" log -1 --format="$format" "$tip" 2>/dev/null
    parents=$(git -C "$repo" rev-parse "$tip^@" 2>/dev/null)
    [ -z "$parents" ] || git -C "$repo" log --since="$since" --format="$format" $parents 2>/dev/null
  done
done
"#,
        since = sh_quote(&since),
        since_unix = sh_quote(&since_unix),
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
