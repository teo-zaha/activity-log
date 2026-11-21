//! A collection run as a script sees it: the exit status, what goes to standard output and what
//! to standard error. Every run uses a config and a cache in a temporary folder and reaches no
//! server but a closed local port.

use std::{
    fs,
    io::Read,
    path::Path,
    process::{Command, Output, Stdio},
};

fn cli(dir: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_activity-log"));
    command
        .env_remove("ACTIVITY_LOG_CONFIG")
        .env("ACTIVITY_LOG_CACHE_DIR", dir.join("cache"))
        .current_dir(dir);
    command
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// A Git connection with no repository to read: it has nothing to give, which skips it.
fn empty_git(dir: &Path) -> String {
    let repos = dir.join("repos");
    fs::create_dir_all(&repos).unwrap();
    format!(
        "[[git]]\nname = \"here\"\nlabel = \"Here\"\ndirs = [{:?}]\n",
        repos.display().to_string()
    )
}

/// A GitLab connection whose server refuses every connection: it fails.
const UNREACHABLE_GITLAB: &str = "[[gitlab]]\nname = \"work\"\nlabel = \"Work\"\nurl = \"http://127.0.0.1:1\"\ntoken = \"not-a-token\"\nuser = \"me\"\n";

fn collect(dir: &Path, config: &str, args: &[&str]) -> Output {
    let path = dir.join("config.toml");
    fs::write(&path, config).unwrap();
    cli(dir)
        .arg("--config")
        .arg(&path)
        .args(["--range", "today"])
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn help_and_version_succeed_on_standard_output() {
    let dir = tempfile::tempdir().unwrap();
    for flag in ["--help", "--version"] {
        let output = cli(dir.path()).arg(flag).output().unwrap();
        assert_eq!(output.status.code(), Some(0), "{flag}");
        assert!(!output.stdout.is_empty(), "{flag}");
    }
    let help = text(&cli(dir.path()).arg("--help").output().unwrap().stdout);
    assert!(help.contains("Exit status"), "{help}");
}

/// A value the CLI does not take is an error like any other: exit 1, the reason on standard
/// error, nothing written.
#[test]
fn an_unknown_value_exits_one() {
    let dir = tempfile::tempdir().unwrap();
    for args in [
        &["--format", "xml"][..],
        &["--range", "fortnight"],
        &["--since", "2026-13-01"],
        &["--no-such-flag"],
    ] {
        let output = cli(dir.path()).args(args).output().unwrap();
        assert_eq!(output.status.code(), Some(1), "{args:?}");
        assert!(output.stdout.is_empty(), "{args:?}");
        assert!(!output.stderr.is_empty(), "{args:?}");
    }
    assert!(!dir.path().join("logs").exists());
}

#[test]
fn a_source_with_nothing_to_give_does_not_fail_the_run() {
    let dir = tempfile::tempdir().unwrap();
    let output = collect(dir.path(), &empty_git(dir.path()), &[]);

    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stdout));
    assert!(dir.path().join("logs/activity_log.json").is_file());
}

/// The others' entries are written, then the run fails.
#[test]
fn a_failed_source_fails_the_run_after_writing_the_export() {
    let dir = tempfile::tempdir().unwrap();
    let config = format!("{}{UNREACHABLE_GITLAB}", empty_git(dir.path()));
    let output = collect(dir.path(), &config, &[]);

    assert_eq!(output.status.code(), Some(1), "{}", text(&output.stdout));
    let export: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.path().join("logs/activity_log.json")).unwrap(),
    )
    .unwrap();
    assert!(export["entries"].is_array());
}

/// With `--output -` standard output holds the export alone, so it can be piped.
#[test]
fn standard_output_holds_only_the_export() {
    let dir = tempfile::tempdir().unwrap();
    let config = format!("{}{UNREACHABLE_GITLAB}", empty_git(dir.path()));
    let output = collect(dir.path(), &config, &["--output", "-"]);

    assert_eq!(output.status.code(), Some(1));
    let export: serde_json::Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|err| panic!("{err}: {}", text(&output.stdout)));
    assert!(export["entries"].is_array());
    assert!(
        text(&output.stderr).contains("gitlab/work"),
        "{}",
        text(&output.stderr)
    );
    assert!(!dir.path().join("logs").exists());
}

#[test]
fn without_sources_standard_output_stays_empty() {
    let dir = tempfile::tempdir().unwrap();
    let output = collect(dir.path(), "", &["--output", "-"]);

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty(), "{}", text(&output.stdout));
    assert!(!output.stderr.is_empty());
}

/// A reader that stops early (`| head -c 1`) ends the run quietly, without a panic.
#[test]
fn a_reader_that_goes_away_ends_the_run_quietly() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, empty_git(dir.path())).unwrap();
    let mut child = cli(dir.path())
        .arg("--config")
        .arg(&path)
        .args(["--range", "today", "--output", "-"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    let status = child.wait().unwrap();

    assert_eq!(status.code(), Some(0), "{stderr}");
    assert!(!stderr.contains("panicked"), "{stderr}");
}

/// What a script reads stops early (`| true`): the commands that print end quietly too.
#[test]
fn listing_into_a_closed_pipe_ends_quietly() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, empty_git(dir.path())).unwrap();
    for args in [&["sources"][..], &["config", "path"]] {
        let mut child = cli(dir.path())
            .arg("--config")
            .arg(&path)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        drop(child.stdout.take());
        let mut stderr = String::new();
        child
            .stderr
            .take()
            .unwrap()
            .read_to_string(&mut stderr)
            .unwrap();
        let status = child.wait().unwrap();

        assert_eq!(status.code(), Some(0), "{args:?}: {stderr}");
        assert!(!stderr.contains("panicked"), "{args:?}: {stderr}");
    }
}

/// `--help` and the README say the same about the exit status.
#[test]
fn the_readme_says_what_help_says_about_the_exit_status() {
    let dir = tempfile::tempdir().unwrap();
    let help = text(&cli(dir.path()).arg("--help").output().unwrap().stdout);
    let readme = include_str!("../../README.md");
    let said = help
        .lines()
        .find(|line| line.starts_with("Exit status:"))
        .expect("--help states the exit status");
    assert!(readme.contains(said), "{said}");
}
