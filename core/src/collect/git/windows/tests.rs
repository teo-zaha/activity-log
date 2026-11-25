use super::{WslCommit, WslOutput, WslRepoReport, WslScan, parse_output, repo_history, repo_name};
use crate::model::CollectOptions;

#[test]
fn parses_repos_commits_and_ignores_other_output() {
    let stdout = "Welcome to NixOS!\n\
        \u{1e}G\u{1f}Me@Example.com\n\
        \u{1e}R\u{1f}/home/me/api\u{1f}me@example.com\n\
        \u{1e}C\u{1f}abc\u{1f}2026-03-05T09:00:00+01:00\u{1f}me@example.com\u{1f}Fix | pipes\u{1f}HEAD -> main, origin/main\n\
        \u{1e}C\u{1f}def\u{1f}2026-03-04T09:00:00+01:00\u{1f}other@example.com\u{1f}Older\u{1f}\n\
        \u{1e}R\u{1f}/home/me/no-email\u{1f}\n";

    assert_eq!(
        parse_output(stdout),
        WslOutput::Scanned(WslScan {
            global_email: Some("Me@Example.com".to_string()),
            repos: vec![
                WslRepoReport {
                    path: "/home/me/api".to_string(),
                    email: Some("me@example.com".to_string()),
                    commits: vec![
                        WslCommit {
                            hash: "abc".to_string(),
                            author_time: "2026-03-05T09:00:00+01:00".to_string(),
                            author_email: "me@example.com".to_string(),
                            subject: "Fix | pipes".to_string(),
                            branch: Some("main".to_string()),
                        },
                        WslCommit {
                            hash: "def".to_string(),
                            author_time: "2026-03-04T09:00:00+01:00".to_string(),
                            author_email: "other@example.com".to_string(),
                            subject: "Older".to_string(),
                            branch: None,
                        },
                    ],
                },
                WslRepoReport {
                    path: "/home/me/no-email".to_string(),
                    email: None,
                    commits: Vec::new(),
                },
            ]
        })
    );
    assert_eq!(repo_name("/home/me/api"), "api");
}

#[test]
fn reports_a_distribution_without_git() {
    assert_eq!(
        parse_output("\u{1e}E\u{1f}git is not installed in this distribution\n"),
        WslOutput::Failed("git is not installed in this distribution".to_string())
    );
}

/// Runs the generated script with the local `sh` and `git`, the way WSL would.
#[cfg(unix)]
#[test]
fn the_script_lists_repos_and_their_history() {
    use std::process::Command;

    use chrono::{TimeZone, Utc};

    use super::{WslTarget, build_script};
    use crate::tests::{git_command, isolate_git};

    let home = tempfile::tempdir().unwrap();
    let repo = home.path().join("work").join("it's here");
    let unconfigured = home.path().join("work").join("unconfigured");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::create_dir_all(&unconfigured).unwrap();
    let git = |dir: &std::path::Path, args: &[&str]| {
        let status = git_command(dir)
            .args(args)
            .env("GIT_AUTHOR_EMAIL", "me@example.com")
            .env("GIT_COMMITTER_EMAIL", "me@example.com")
            .env("GIT_AUTHOR_DATE", "2026-03-05T09:00:00+01:00")
            .env("GIT_COMMITTER_DATE", "2026-03-05T09:00:00+01:00")
            .status()
            .unwrap();
        assert!(status.success());
    };
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "user.email", "me@example.com"]);
    git(&repo, &["config", "user.name", "Me"]);
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "First"]);
    git(&unconfigured, &["init", "-q", "-b", "main"]);
    git(
        &unconfigured,
        &["commit", "-q", "--allow-empty", "-m", "Without config"],
    );
    // A worktree shares the repository's history and must not be read twice.
    git(
        &repo,
        &["worktree", "add", "-q", "../worktree", "-b", "side"],
    );

    let target = WslTarget {
        distro: "Test".to_string(),
        dirs: vec![
            "~".to_string(),
            "~/work".to_string(),
            repo.display().to_string(),
        ],
    };
    let global_config = home.path().join("gitconfig");
    let run = |since| {
        let mut command = Command::new("sh");
        let output = isolate_git(&mut command)
            .env("HOME", home.path())
            .env("GIT_CONFIG_GLOBAL", &global_config)
            .arg("-c")
            .arg(build_script(&target, since))
            .output()
            .unwrap();
        parse_output(&String::from_utf8_lossy(&output.stdout))
    };

    let WslOutput::Scanned(listed) = run(None) else {
        panic!("listing failed");
    };
    assert_eq!(
        listed.repos.len(),
        1,
        "the check stops at the first repository"
    );
    assert!(listed.repos.iter().all(|repo| repo.commits.is_empty()));
    assert_eq!(listed.global_email, None);

    let since = Some(Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap());
    let WslOutput::Scanned(collected) = run(since) else {
        panic!("collecting failed");
    };
    let [configured, unconfigured] = collected.repos.as_slice() else {
        panic!("expected two repositories: {collected:?}");
    };
    assert_eq!(configured.email.as_deref(), Some("me@example.com"));
    assert_eq!(configured.commits[0].subject, "First");
    assert_eq!(configured.commits[0].branch.as_deref(), Some("main"));
    assert_eq!(
        configured.commits[0].author_time,
        "2026-03-05T09:00:00+01:00"
    );
    assert_eq!(unconfigured.email, None);
    assert_eq!(
        unconfigured.commits[0].subject, "Without config",
        "history is printed for repositories without user.email too"
    );

    std::fs::write(&global_config, "[user]\n\temail = global@example.com\n").unwrap();
    let WslOutput::Scanned(collected) = run(since) else {
        panic!("collecting failed");
    };
    assert_eq!(
        collected.global_email.as_deref(),
        Some("global@example.com")
    );
}

#[test]
fn repo_history_keeps_every_author_in_the_range_and_labels_the_distro() {
    let commit = |hash: &str, author_time: &str, author_email: &str| WslCommit {
        hash: hash.to_string(),
        author_time: author_time.to_string(),
        author_email: author_email.to_string(),
        subject: hash.to_string(),
        branch: None,
    };
    let report = WslRepoReport {
        path: "/home/me/api".to_string(),
        email: None,
        commits: vec![
            commit("mine", "2026-03-05T09:00:00+01:00", "me@example.com"),
            commit("theirs", "2026-03-05T10:00:00+01:00", "other@example.com"),
            // Committed in the range (so `--since` printed it) but authored before.
            commit("rebased", "2026-02-27T09:00:00+01:00", "me@example.com"),
        ],
    };
    let options = CollectOptions {
        start: "2026-03-01T00:00:00Z".parse().unwrap(),
        end: "2026-04-01T00:00:00Z".parse().unwrap(),
    };

    let history = repo_history("NixOS", report, &options);

    assert_eq!(history.label, "NixOS:api");
    assert_eq!(history.email, None);
    assert_eq!(
        history
            .commits
            .iter()
            .map(|commit| (commit.repo.as_str(), commit.hash.as_str()))
            .collect::<Vec<_>>(),
        vec![("api", "mine"), ("api", "theirs")]
    );
}

/// As on this machine: a tip recorded before `since` is read with the history behind it.
#[cfg(unix)]
#[test]
fn the_script_reads_a_tip_recorded_before_since_with_its_parents() {
    use std::process::Command;

    use chrono::{TimeZone, Utc};

    use super::{WslTarget, build_script};
    use crate::tests::{git_command, isolate_git};

    let home = tempfile::tempdir().unwrap();
    let repo = home.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let git = |args: &[&str], authored: &str, committed: &str| {
        let status = git_command(&repo)
            .args(args)
            .env("GIT_AUTHOR_EMAIL", "me@example.com")
            .env("GIT_COMMITTER_EMAIL", "me@example.com")
            .env("GIT_AUTHOR_DATE", authored)
            .env("GIT_COMMITTER_DATE", committed)
            .status()
            .unwrap();
        assert!(status.success());
    };
    let now = "2026-03-10T09:00:00+00:00";
    git(&["init", "-q", "-b", "main"], now, now);
    git(&["config", "user.email", "me@example.com"], now, now);
    git(&["config", "user.name", "Me"], now, now);
    git(&["commit", "-q", "--allow-empty", "-m", "Parent"], now, now);
    git(
        &["commit", "-q", "--allow-empty", "-m", "Old tip"],
        "2026-03-11T09:00:00+00:00",
        "2026-01-01T09:00:00+00:00",
    );

    let target = WslTarget {
        distro: "Test".to_string(),
        dirs: vec![repo.display().to_string()],
    };
    let mut command = Command::new("sh");
    let output = isolate_git(&mut command)
        .arg("-c")
        // `since` as the collector passes it: the range's start less the margin.
        .arg(build_script(
            &target,
            Some(Utc.with_ymd_and_hms(2026, 2, 1, 0, 0, 0).unwrap()),
        ))
        .output()
        .unwrap();
    let WslOutput::Scanned(scan) = parse_output(&String::from_utf8_lossy(&output.stdout)) else {
        panic!("collecting failed");
    };

    let options = CollectOptions {
        start: Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap(),
        end: Utc.with_ymd_and_hms(2026, 3, 12, 0, 0, 0).unwrap(),
    };
    let history = repo_history("Test", scan.repos.into_iter().next().unwrap(), &options);
    let subjects = history
        .commits
        .iter()
        .map(|commit| commit.message.as_str())
        .collect::<Vec<_>>();
    assert_eq!(subjects, ["Old tip", "Parent"], "each once");
}

/// The script's git runs in the C locale whatever the distribution's language: a stand-in `git`
/// reports the locale it was started in.
#[cfg(unix)]
#[test]
fn the_script_runs_git_in_the_c_locale() {
    use std::{os::unix::fs::PermissionsExt, process::Command};

    use super::{WslTarget, build_script};
    use crate::tests::isolate_git;

    let bin = tempfile::tempdir().unwrap();
    let seen = bin.path().join("locale");
    let git = bin.path().join("git");
    std::fs::write(
        &git,
        format!(
            "#!/bin/sh\nprintf '%s' \"$LC_ALL\" > '{}'\n",
            seen.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&git, std::fs::Permissions::from_mode(0o755)).unwrap();
    let target = WslTarget {
        distro: "Test".to_string(),
        dirs: vec![bin.path().display().to_string()],
    };

    isolate_git(&mut Command::new("/bin/sh"))
        .arg("-c")
        .arg(build_script(&target, Some(chrono::Utc::now())))
        .env("PATH", format!("{}:/usr/bin:/bin", bin.path().display()))
        .env("LC_ALL", "de_DE.UTF-8")
        .output()
        .unwrap();

    assert_eq!(std::fs::read_to_string(&seen).unwrap(), "C");
}
