use std::{fs, path::Path, process::Command};

use chrono::{DateTime, Days, NaiveDate, TimeZone, Utc};
use tempfile::{TempDir, tempdir};

use crate::{model::CollectOptions, provider::git::GitConfig};

use super::{RepoCommits, RepoHistory, collect_git, own_commits};
use crate::{model::GitEntry, provider::connection::ConnectionLocation};

/// Collects one local connection and keeps the user's commits, newest first, as the provider
/// does across its connections.
async fn collect(options: &CollectOptions, config: &GitConfig) -> anyhow::Result<Vec<GitEntry>> {
    let commits = collect_git(options, config, ConnectionLocation::Local).await?;
    let (_, mut own, _) = own_commits(commits);
    own.sort_by(|a, b| {
        DateTime::parse_from_rfc3339(&b.timestamp)
            .ok()
            .cmp(&DateTime::parse_from_rfc3339(&a.timestamp).ok())
            .then_with(|| a.repo.cmp(&b.repo))
            .then_with(|| a.hash.cmp(&b.hash))
    });
    Ok(own)
}

/// Whole UTC days `since..=until`.
fn options(since: &str, until: &str) -> CollectOptions {
    let day = |value: &str| NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap();
    CollectOptions {
        start: day(since).and_hms_opt(0, 0, 0).unwrap().and_utc(),
        end: (day(until) + Days::new(1))
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc(),
    }
}

fn utc_time(year: i32, month: u32, day: u32, hour: u32, minute: u32, second: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(year, month, day, hour, minute, second)
        .single()
        .unwrap()
}

fn init_repo() -> TempDir {
    let dir = tempdir().expect("tempdir");
    run_git(dir.path(), &["init"]);
    run_git(dir.path(), &["checkout", "-b", "main"]);
    run_git(dir.path(), &["config", "user.name", "Activity Log"]);
    dir
}

fn set_repo_email(repo: &Path, email: &str) {
    run_git(repo, &["config", "user.email", email]);
}

fn commit_file(
    repo: &Path,
    file_name: &str,
    contents: &str,
    subject: &str,
    body: Option<&str>,
    author_email: &str,
    when: DateTime<Utc>,
) {
    fs::write(repo.join(file_name), contents).expect("write file");
    run_git(repo, &["add", "."]);

    let mut args = vec!["commit", "-m", subject];
    if let Some(body) = body {
        args.push("-m");
        args.push(body);
    }

    run_git_with_env(
        repo,
        &args,
        &[
            ("GIT_AUTHOR_NAME", "Activity Log".to_string()),
            ("GIT_AUTHOR_EMAIL", author_email.to_string()),
            ("GIT_AUTHOR_DATE", git_time(when)),
            ("GIT_COMMITTER_NAME", "Activity Log".to_string()),
            ("GIT_COMMITTER_EMAIL", author_email.to_string()),
            ("GIT_COMMITTER_DATE", git_time(when)),
        ],
    );
}

fn detach_head(repo: &Path) {
    run_git(repo, &["checkout", "--detach", "HEAD"]);
}

fn git_time(value: DateTime<Utc>) -> String {
    value.format("%Y-%m-%dT%H:%M:%S%:z").to_string()
}

fn run_git(repo: &Path, args: &[&str]) {
    run_git_with_env(repo, args, &[]);
}

fn run_git_with_env(repo: &Path, args: &[&str], envs: &[(&str, String)]) {
    let mut command = Command::new("git");
    command.args(args).current_dir(repo);
    for (key, value) in envs {
        command.env(key, value);
    }

    let output = command.output().expect("run git");
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
async fn collects_matching_commits_and_only_sets_branch_on_head_commit() {
    let repo = init_repo();
    set_repo_email(repo.path(), "me@example.com");
    commit_file(
        repo.path(),
        "first.txt",
        "first",
        "First subject",
        Some("First body"),
        "me@example.com",
        utc_time(2026, 3, 5, 9, 0, 0),
    );
    commit_file(
        repo.path(),
        "second.txt",
        "second",
        "Second subject",
        None,
        "me@example.com",
        utc_time(2026, 3, 6, 10, 30, 0),
    );

    let entries = collect(
        &options("2026-03-01", "2026-03-31"),
        &GitConfig::with_dirs(vec![repo.path().to_path_buf()]),
    )
    .await
    .unwrap();

    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].message, "Second subject");
    assert_eq!(entries[0].branch, "main");
    assert_eq!(entries[1].message, "First subject");
    assert_eq!(entries[1].branch, "");
}

#[tokio::test]
async fn excludes_commits_with_non_matching_author_email() {
    let repo = init_repo();
    set_repo_email(repo.path(), "me@example.com");
    commit_file(
        repo.path(),
        "note.txt",
        "note",
        "Other author",
        None,
        "other@example.com",
        utc_time(2026, 3, 5, 9, 0, 0),
    );

    let entries = collect(
        &options("2026-03-01", "2026-03-31"),
        &GitConfig::with_dirs(vec![repo.path().to_path_buf()]),
    )
    .await
    .unwrap();

    assert!(entries.is_empty());
}

#[tokio::test]
async fn repositories_without_user_email_match_the_emails_of_the_others() {
    let configured = init_repo();
    set_repo_email(configured.path(), "me@example.com");
    let unconfigured = init_repo();
    commit_file(
        unconfigured.path(),
        "note.txt",
        "note",
        "No config email",
        None,
        "Me@Example.com",
        utc_time(2026, 3, 5, 9, 0, 0),
    );
    commit_file(
        unconfigured.path(),
        "other.txt",
        "other",
        "Someone else",
        None,
        "other@example.com",
        utc_time(2026, 3, 5, 10, 0, 0),
    );

    let entries = collect(
        &options("2026-03-01", "2026-03-31"),
        &GitConfig::with_dirs(vec![
            configured.path().to_path_buf(),
            unconfigured.path().to_path_buf(),
        ]),
    )
    .await
    .unwrap();

    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.message.as_str())
            .collect::<Vec<_>>(),
        vec!["No config email"]
    );
}

fn entry(repo: &str, hash: &str, author: &str) -> GitEntry {
    GitEntry::new(
        repo.to_string(),
        hash.to_string(),
        hash.to_string(),
        author.to_string(),
        String::new(),
        "2026-03-05T09:00:00+01:00".to_string(),
    )
}

fn history(label: &str, email: Option<&str>, commits: Vec<GitEntry>) -> RepoHistory {
    RepoHistory {
        label: label.to_string(),
        email: email.map(str::to_string),
        commits,
    }
}

#[test]
fn own_commits_match_every_known_email_case_insensitively() {
    let (found, own, ignored) = own_commits(RepoCommits {
        repos: vec![
            history(
                "api",
                Some("Work@Example.com"),
                vec![
                    entry("api", "work", "work@example.com"),
                    entry("api", "private", "me@example.org"),
                    entry("api", "colleague", "colleague@example.com"),
                ],
            ),
            history(
                "NixOS:dotfiles",
                None,
                vec![entry("dotfiles", "dots", "ME@example.org")],
            ),
        ],
        identities: vec![" me@example.org ".to_string(), String::new()],
    });

    assert_eq!(
        own.iter()
            .map(|entry| entry.hash.as_str())
            .collect::<Vec<_>>(),
        vec!["work", "private", "dots"]
    );
    assert_eq!(
        found.identities.into_iter().collect::<Vec<_>>(),
        vec!["me@example.org", "work@example.com"]
    );
    assert_eq!(found.without_email, vec!["NixOS:dotfiles"]);
    assert_eq!(ignored, 1);
}

#[test]
fn own_commits_keep_nothing_without_any_identity() {
    let (found, own, ignored) = own_commits(RepoCommits {
        repos: vec![
            history("api", None, vec![entry("api", "a", "me@example.com")]),
            history("web", None, Vec::new()),
        ],
        identities: Vec::new(),
    });

    assert!(own.is_empty());
    assert!(found.identities.is_empty());
    assert_eq!(found.without_email, vec!["api", "web"]);
    assert_eq!(ignored, 1);
}

#[tokio::test]
async fn detached_head_yields_empty_branch() {
    let repo = init_repo();
    set_repo_email(repo.path(), "me@example.com");
    commit_file(
        repo.path(),
        "note.txt",
        "note",
        "Detached commit",
        None,
        "me@example.com",
        utc_time(2026, 3, 5, 9, 0, 0),
    );
    detach_head(repo.path());

    let entries = collect(
        &options("2026-03-01", "2026-03-31"),
        &GitConfig::with_dirs(vec![repo.path().to_path_buf()]),
    )
    .await
    .unwrap();

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].branch, "");
}

#[tokio::test]
async fn commit_message_uses_subject_only() {
    let repo = init_repo();
    set_repo_email(repo.path(), "me@example.com");
    commit_file(
        repo.path(),
        "note.txt",
        "note",
        "Subject line",
        Some("Body paragraph"),
        "me@example.com",
        utc_time(2026, 3, 5, 9, 0, 0),
    );

    let entries = collect(
        &options("2026-03-01", "2026-03-31"),
        &GitConfig::with_dirs(vec![repo.path().to_path_buf()]),
    )
    .await
    .unwrap();

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].message, "Subject line");
}

#[tokio::test]
async fn deduplicates_commits_reachable_from_multiple_refs() {
    let repo = init_repo();
    set_repo_email(repo.path(), "me@example.com");
    commit_file(
        repo.path(),
        "note.txt",
        "note",
        "Tagged commit",
        None,
        "me@example.com",
        utc_time(2026, 3, 5, 9, 0, 0),
    );
    run_git(repo.path(), &["tag", "v1"]);
    run_git(repo.path(), &["branch", "copy"]);

    let entries = collect(
        &options("2026-03-01", "2026-03-31"),
        &GitConfig::with_dirs(vec![repo.path().to_path_buf()]),
    )
    .await
    .unwrap();

    assert_eq!(entries.len(), 1);
}

#[tokio::test]
async fn includes_commits_on_date_window_boundaries_and_excludes_outside() {
    let repo = init_repo();
    set_repo_email(repo.path(), "me@example.com");
    commit_file(
        repo.path(),
        "before.txt",
        "before",
        "Before window",
        None,
        "me@example.com",
        utc_time(2026, 3, 9, 23, 59, 59),
    );
    commit_file(
        repo.path(),
        "start.txt",
        "start",
        "At start",
        None,
        "me@example.com",
        utc_time(2026, 3, 10, 0, 0, 0),
    );
    commit_file(
        repo.path(),
        "end.txt",
        "end",
        "At end",
        None,
        "me@example.com",
        utc_time(2026, 3, 11, 23, 59, 59),
    );
    commit_file(
        repo.path(),
        "after.txt",
        "after",
        "After window",
        None,
        "me@example.com",
        utc_time(2026, 3, 12, 0, 0, 0),
    );

    let entries = collect(
        &options("2026-03-10", "2026-03-11"),
        &GitConfig::with_dirs(vec![repo.path().to_path_buf()]),
    )
    .await
    .unwrap();

    let messages = entries
        .iter()
        .map(|entry| entry.message.as_str())
        .collect::<Vec<_>>();
    assert_eq!(messages, vec!["At end", "At start"]);
}

#[tokio::test]
async fn skips_invalid_repositories_but_keeps_collecting_valid_ones() {
    let repo = init_repo();
    set_repo_email(repo.path(), "me@example.com");
    commit_file(
        repo.path(),
        "valid.txt",
        "valid",
        "Valid repo",
        None,
        "me@example.com",
        utc_time(2026, 3, 5, 9, 0, 0),
    );

    let invalid_root = tempdir().unwrap();
    fs::create_dir_all(invalid_root.path().join(".git")).unwrap();

    let entries = collect(
        &options("2026-03-01", "2026-03-31"),
        &GitConfig::with_dirs(vec![
            invalid_root.path().to_path_buf(),
            repo.path().to_path_buf(),
        ]),
    )
    .await
    .unwrap();

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].message, "Valid repo");
}

#[tokio::test]
async fn reads_a_repository_once_however_many_worktrees_it_has() {
    let root = tempdir().unwrap();
    let main = root.path().join("project");
    fs::create_dir_all(&main).unwrap();
    run_git(&main, &["init", "-b", "main"]);
    set_repo_email(&main, "me@example.com");
    commit_file(
        &main,
        "a.txt",
        "a",
        "Shared commit",
        None,
        "me@example.com",
        utc_time(2026, 3, 5, 9, 0, 0),
    );
    run_git(
        &main,
        &[
            "worktree",
            "add",
            "-q",
            "../project-feature",
            "-b",
            "feature",
        ],
    );

    let entries = collect(
        &options("2026-03-01", "2026-03-31"),
        &GitConfig::with_dirs(vec![root.path().to_path_buf()]),
    )
    .await
    .unwrap();

    assert_eq!(entries.len(), 1, "{entries:?}");
    assert_eq!(entries[0].repo, "project");
}

/// Folders that each look like a repository, for the walk.
fn folders_with_repos(count: usize) -> TempDir {
    let root = tempdir().unwrap();
    for index in 0..count {
        fs::create_dir_all(root.path().join(format!("group/repo-{index}/.git"))).unwrap();
    }
    root
}

#[test]
fn a_walk_ends_once_it_is_cancelled() {
    use std::{
        ops::ControlFlow,
        sync::atomic::{AtomicBool, Ordering},
    };

    let root = folders_with_repos(20);
    let cancelled = AtomicBool::new(false);
    let mut visited = 0;

    let walked = super::visit_git_dir_input(root.path(), &cancelled, &mut |_| {
        visited += 1;
        // The run is stopped while the walk is at its second repository.
        if visited == 2 {
            cancelled.store(true, Ordering::Relaxed);
        }
        ControlFlow::Continue(())
    });

    assert!(walked.is_break());
    assert_eq!(visited, 2);
}

#[tokio::test]
async fn a_cancelled_local_collection_reads_no_repository() {
    use std::sync::atomic::AtomicBool;

    let root = folders_with_repos(3);
    let config = GitConfig {
        dirs: Some(vec![root.path().to_path_buf()]),
    };

    let error = super::collect_local_repos(
        &options("2026-03-01", "2026-03-31"),
        &config,
        &AtomicBool::new(true),
    )
    .unwrap_err();

    assert!(error.to_string().starts_with("Stopped"));
}

/// A commit recorded before it was authored (a clock off, a rebase that keeps the dates), and
/// one in the range that only a commit recorded before the range leads to: both are work of
/// the range. History is pruned by commit time, so it is read with a margin before the range.
#[tokio::test]
async fn finds_authored_work_whose_commit_time_is_before_the_range() {
    let repo = init_repo();
    set_repo_email(repo.path(), "me@example.com");
    let commit_at =
        |file: &str, subject: &str, authored: DateTime<Utc>, committed: DateTime<Utc>| {
            fs::write(repo.path().join(file), subject).unwrap();
            run_git(repo.path(), &["add", "."]);
            run_git_with_env(
                repo.path(),
                &["commit", "-m", subject],
                &[
                    ("GIT_AUTHOR_EMAIL", "me@example.com".to_string()),
                    ("GIT_AUTHOR_DATE", git_time(authored)),
                    ("GIT_COMMITTER_EMAIL", "me@example.com".to_string()),
                    ("GIT_COMMITTER_DATE", git_time(committed)),
                ],
            );
        };
    commit_at(
        "a.txt",
        "In the range",
        utc_time(2026, 3, 10, 9, 0, 0),
        utc_time(2026, 3, 10, 9, 0, 0),
    );
    // Its child, the branch's tip, was recorded with an older date.
    commit_at(
        "b.txt",
        "Recorded early",
        utc_time(2026, 3, 11, 9, 0, 0),
        utc_time(2026, 3, 1, 9, 0, 0),
    );

    let config = GitConfig::with_dirs(vec![repo.path().to_path_buf()]);
    let entries = collect(&options("2026-03-10", "2026-03-12"), &config)
        .await
        .unwrap();

    let subjects = entries
        .iter()
        .map(|entry| entry.message.as_str())
        .collect::<Vec<_>>();
    assert_eq!(subjects, ["Recorded early", "In the range"]);
}

/// A branch tip recorded long before the range (more than the margin) still leads to the
/// commits behind it, and counts by its own author time.
#[tokio::test]
async fn a_tip_recorded_long_before_the_range_still_leads_to_its_history() {
    let repo = init_repo();
    set_repo_email(repo.path(), "me@example.com");
    for (file, subject, authored, committed) in [
        (
            "a.txt",
            "Parent in the range",
            utc_time(2026, 3, 10, 9, 0, 0),
            utc_time(2026, 3, 10, 9, 0, 0),
        ),
        (
            "b.txt",
            "Tip recorded in February",
            utc_time(2026, 3, 11, 9, 0, 0),
            utc_time(2026, 2, 1, 9, 0, 0),
        ),
    ] {
        fs::write(repo.path().join(file), subject).unwrap();
        run_git(repo.path(), &["add", "."]);
        run_git_with_env(
            repo.path(),
            &["commit", "-m", subject],
            &[
                ("GIT_AUTHOR_EMAIL", "me@example.com".to_string()),
                ("GIT_AUTHOR_DATE", git_time(authored)),
                ("GIT_COMMITTER_EMAIL", "me@example.com".to_string()),
                ("GIT_COMMITTER_DATE", git_time(committed)),
            ],
        );
    }

    let config = GitConfig::with_dirs(vec![repo.path().to_path_buf()]);
    let entries = collect(&options("2026-03-10", "2026-03-12"), &config)
        .await
        .unwrap();

    let subjects = entries
        .iter()
        .map(|entry| entry.message.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        subjects,
        ["Tip recorded in February", "Parent in the range"]
    );
}
