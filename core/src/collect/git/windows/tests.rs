use super::{
    WslCommit, WslOutput, WslRepoReport, WslScan, WslTarget, parse_output, repo_history, repo_name,
    wsl_targets_with,
};
use crate::model::CollectOptions;
use crate::provider::git::{
    GitConfig,
    windows::{WslConfig, WslRepo},
};

fn config(wsl_distros: Option<Vec<&str>>, wsl_repos: &[(&str, &str)]) -> GitConfig {
    GitConfig {
        dirs: None,
        wsl: WslConfig {
            distros: wsl_distros.map(|names| names.into_iter().map(str::to_string).collect()),
            repos: wsl_repos
                .iter()
                .map(|(distro, path)| WslRepo {
                    distro: distro.to_string(),
                    path: path.to_string(),
                })
                .collect(),
        },
    }
}

#[test]
fn unset_distros_fall_back_to_the_default_and_explicit_repos_join_them() {
    let targets = wsl_targets_with(
        &config(None, &[("NixOS", "/home/me/api"), ("Debian", "/srv/tool")]),
        || vec!["NixOS".to_string()],
    );

    assert_eq!(
        targets,
        vec![
            WslTarget {
                distro: "Debian".to_string(),
                discover: false,
                repos: vec!["/srv/tool".to_string()],
            },
            WslTarget {
                distro: "NixOS".to_string(),
                discover: true,
                repos: vec!["/home/me/api".to_string()],
            },
        ]
    );
}

#[test]
fn an_empty_distro_list_searches_nothing() {
    let targets = wsl_targets_with(&config(Some(Vec::new()), &[]), || {
        panic!("the default must not be read when distros are configured")
    });

    assert!(targets.is_empty());
}

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
                            branch: "main".to_string(),
                        },
                        WslCommit {
                            hash: "def".to_string(),
                            author_time: "2026-03-04T09:00:00+01:00".to_string(),
                            author_email: "other@example.com".to_string(),
                            subject: "Older".to_string(),
                            branch: String::new(),
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

    use super::build_script;

    let home = tempfile::tempdir().unwrap();
    let repo = home.path().join("work").join("it's here");
    let unconfigured = home.path().join("work").join("unconfigured");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::create_dir_all(&unconfigured).unwrap();
    // Isolated from the developer's own git config, like a fresh distribution.
    let isolated = |command: &mut Command| {
        command
            .env("HOME", home.path())
            .env("XDG_CONFIG_HOME", home.path().join(".config"))
            .env("GIT_CONFIG_NOSYSTEM", "1");
    };
    let git = |dir: &std::path::Path, args: &[&str]| {
        let mut command = Command::new("git");
        isolated(&mut command);
        let status = command
            .args(args)
            .current_dir(dir)
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
        &[
            "-c",
            "user.email=me@example.com",
            "-c",
            "user.name=Me",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "Without config",
        ],
    );

    let target = WslTarget {
        distro: "Test".to_string(),
        discover: true,
        repos: vec![repo.display().to_string()],
    };
    let run = |since| {
        let mut command = Command::new("sh");
        isolated(&mut command);
        let output = command
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
        2,
        "discovered and explicit paths are merged"
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
    assert_eq!(configured.commits[0].branch, "main");
    assert_eq!(
        configured.commits[0].author_time,
        "2026-03-05T09:00:00+01:00"
    );
    assert_eq!(unconfigured.email, None);
    assert_eq!(
        unconfigured.commits[0].subject, "Without config",
        "history is printed for repositories without user.email too"
    );

    git(
        &repo,
        &["config", "--global", "user.email", "global@example.com"],
    );
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
        branch: String::new(),
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
