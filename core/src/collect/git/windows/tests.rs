use super::{
    WslCommit, WslOutput, WslRepoReport, WslTarget, parse_output, repo_name, wsl_targets_with,
};
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
        \u{1e}R\u{1f}/home/me/api\u{1f}me@example.com\n\
        \u{1e}C\u{1f}abc\u{1f}2026-03-05T09:00:00+01:00\u{1f}me@example.com\u{1f}Fix | pipes\u{1f}HEAD -> main, origin/main\n\
        \u{1e}C\u{1f}def\u{1f}2026-03-04T09:00:00+01:00\u{1f}other@example.com\u{1f}Older\u{1f}\n\
        \u{1e}R\u{1f}/home/me/no-email\u{1f}\n";

    assert_eq!(
        parse_output(stdout),
        WslOutput::Repos(vec![
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
        ])
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
    std::fs::create_dir_all(&repo).unwrap();
    let git = |args: &[&str]| {
        let status = Command::new("git")
            .args(args)
            .current_dir(&repo)
            .env("GIT_AUTHOR_DATE", "2026-03-05T09:00:00+01:00")
            .env("GIT_COMMITTER_DATE", "2026-03-05T09:00:00+01:00")
            .status()
            .unwrap();
        assert!(status.success());
    };
    git(&["init", "-q", "-b", "main"]);
    git(&["config", "user.email", "me@example.com"]);
    git(&["config", "user.name", "Me"]);
    git(&["commit", "-q", "--allow-empty", "-m", "First"]);

    let target = WslTarget {
        distro: "Test".to_string(),
        discover: true,
        repos: vec![repo.display().to_string()],
    };
    let run = |since| {
        let output = Command::new("sh")
            .arg("-c")
            .arg(build_script(&target, since))
            .env("HOME", home.path())
            .output()
            .unwrap();
        parse_output(&String::from_utf8_lossy(&output.stdout))
    };

    let WslOutput::Repos(listed) = run(None) else {
        panic!("listing failed");
    };
    assert_eq!(listed.len(), 1, "discovered and explicit paths are merged");
    assert!(listed[0].commits.is_empty());

    let WslOutput::Repos(collected) = run(Some(Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap()))
    else {
        panic!("collecting failed");
    };
    assert_eq!(collected[0].email.as_deref(), Some("me@example.com"));
    assert_eq!(collected[0].commits[0].subject, "First");
    assert_eq!(collected[0].commits[0].branch, "main");
    assert_eq!(
        collected[0].commits[0].author_time,
        "2026-03-05T09:00:00+01:00"
    );
}
