use std::{
    collections::{BTreeSet, HashSet},
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Result, anyhow};
use chrono::{DateTime, FixedOffset, Local, NaiveDate, TimeZone, Utc};
use gix::{
    ObjectId, Repository,
    bstr::{BStr, ByteSlice},
    open::Options as OpenOptions,
    revision::walk::Sorting,
};

use crate::{
    model::{CollectOptions, GitEntry},
    provider::git::{GitConfig, default_git_root},
};

#[cfg(test)]
mod tests;

pub async fn collect_git(options: &CollectOptions, config: &GitConfig) -> Result<Vec<GitEntry>> {
    let dirs = resolve_git_repos(config)?;
    let (start, end) = utc_bounds(options.since, options.until)?;
    let mut entries = Vec::new();

    for repo_path in dirs {
        let repo_name = repo_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string();
        let Ok(repo) = open_repo_strict(&repo_path) else {
            continue;
        };
        let Some(author_email) = repo_user_email(&repo) else {
            continue;
        };
        let Ok(mut repo_entries) =
            collect_repo_entries(&repo, &repo_name, &author_email, start, end)
        else {
            continue;
        };
        entries.append(&mut repo_entries);
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

    Ok(entries)
}

pub(crate) fn open_repo_strict(path: &Path) -> Result<Repository> {
    gix::open_opts(path, OpenOptions::default().bail_if_untrusted(true)).map_err(Into::into)
}

pub(crate) fn repo_user_email(repo: &Repository) -> Option<String> {
    let value = repo.config_snapshot().string("user.email")?;
    let email = bstr_to_string(value.as_ref()).trim().to_string();
    (!email.is_empty()).then_some(email)
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

fn collect_repo_entries(
    repo: &Repository,
    repo_name: &str,
    author_email: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<Vec<GitEntry>> {
    let head = head_target(repo);
    let tips = reachable_ref_tips(repo)?;
    let mut entries = Vec::new();

    let walk = repo
        .rev_walk(tips)
        .sorting(Sorting::ByCommitTime(Default::default()))
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
        if timestamp_utc < start || timestamp_utc > end {
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

fn utc_bounds(since: NaiveDate, until: NaiveDate) -> Result<(DateTime<Utc>, DateTime<Utc>)> {
    let start = Local
        .from_local_datetime(
            &since
                .and_hms_opt(0, 0, 0)
                .ok_or_else(|| anyhow!("invalid since date"))?,
        )
        .single()
        .ok_or_else(|| anyhow!("invalid since date"))?
        .with_timezone(&Utc);
    let end = Local
        .from_local_datetime(
            &until
                .and_hms_opt(23, 59, 59)
                .ok_or_else(|| anyhow!("invalid until date"))?,
        )
        .single()
        .ok_or_else(|| anyhow!("invalid until date"))?
        .with_timezone(&Utc);

    Ok((start, end))
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
