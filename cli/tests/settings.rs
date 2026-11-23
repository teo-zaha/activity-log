//! The CLI as another process writing the settings: the config lock between processes, and
//! the behaviour without a settings location. Every run uses a config in a temporary folder,
//! never the user's.

use std::{
    fs::{self, File},
    path::Path,
    process::{Command, Output, Stdio},
};

/// The CLI run in `dir`, without the `ACTIVITY_LOG_CONFIG` of the environment the tests run in.
/// Each test passes its own config or clears the environment.
fn cli(dir: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_activity-log"));
    command.env_remove("ACTIVITY_LOG_CONFIG").current_dir(dir);
    command
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

const WORK: &str = "[[gitlab]]\nname = \"work\"\nlabel = \"Work\"\n";

#[test]
fn a_second_process_is_busy_while_the_lock_is_held() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, WORK).unwrap();
    let held = File::create(dir.path().join("config.toml.lock")).unwrap();
    held.lock().unwrap();

    let output = cli(dir.path())
        .env("ACTIVITY_LOG_LOCK_WAIT_MS", "200")
        .arg("--config")
        .arg(&path)
        .args(["sources", "set", "gitlab/work", "url", "https://gitlab.com"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("The settings file is busy; try again."),
        "{}",
        stderr(&output)
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), WORK);
}

/// Both processes read the file before the lock is free; each must apply its change to the
/// file as it is under the lock, so the second sees the first's.
#[test]
fn two_processes_setting_different_fields_both_land() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, WORK).unwrap();
    let held = File::create(dir.path().join("config.toml.lock")).unwrap();
    held.lock().unwrap();
    let set = |field: &str, value: &str| {
        cli(dir.path())
            .arg("--config")
            .arg(&path)
            .args(["sources", "set", "gitlab/work", field, value])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap()
    };

    let children = [set("url", "https://gitlab.com"), set("user", "me")];
    // Long enough for both to have read the file and to wait for the lock.
    std::thread::sleep(std::time::Duration::from_millis(500));
    drop(held);
    for mut child in children {
        assert!(child.wait().unwrap().success());
    }

    let written = fs::read_to_string(&path).unwrap();
    assert!(
        written.contains("url = \"https://gitlab.com\""),
        "{written}"
    );
    assert!(written.contains("user = \"me\""), "{written}");
}

/// No `HOME`, `APPDATA` or `XDG_CONFIG_HOME`: nothing is read or written anywhere, not even
/// in the temporary folder the old fallback used.
#[cfg(unix)]
#[test]
fn without_a_settings_location_nothing_is_written() {
    const NO_LOCATION: &str = "Activity Log has no folder for its settings. Set ACTIVITY_LOG_CONFIG to a file path, or start it with --config <path>.";
    let dir = tempfile::tempdir().unwrap();
    let temp = dir.path().join("temp");
    fs::create_dir(&temp).unwrap();
    let run = |args: &[&str]| {
        cli(dir.path())
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("TMPDIR", &temp)
            .env("TEMP", &temp)
            .env("TMP", &temp)
            .args(args)
            .output()
            .unwrap()
    };

    for args in [
        &["config", "path"][..],
        &["config", "edit"],
        &["sources", "set", "gitlab/work", "url", "https://gitlab.com"],
        &["sources", "add", "git"],
    ] {
        let output = run(args);
        assert_eq!(output.status.code(), Some(1), "{args:?}");
        assert!(
            stderr(&output).contains(NO_LOCATION),
            "{args:?}: {}",
            stderr(&output)
        );
    }
    let listed = run(&["sources"]);
    assert!(listed.status.success());
    assert!(stderr(&listed).contains(NO_LOCATION));

    assert_eq!(fs::read_dir(&temp).unwrap().count(), 0);
    let mut names = fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect::<Vec<_>>();
    names.sort();
    assert_eq!(names, ["temp"]);
}

#[test]
fn a_relative_config_path_is_used_from_the_working_folder() {
    let dir = tempfile::tempdir().unwrap();

    let output = cli(dir.path())
        .args(["--config", "config.toml", "sources", "add", "system"])
        .output()
        .unwrap();

    assert!(output.status.success(), "{}", stderr(&output));
    let written = fs::read_to_string(dir.path().join("config.toml")).unwrap();
    assert!(written.starts_with("[[system]]\n"), "{written}");
}

#[cfg(unix)]
#[test]
fn config_edit_without_a_change_creates_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");

    let output = cli(dir.path())
        .env("VISUAL", "true")
        .env("EDITOR", "true")
        .arg("--config")
        .arg(&path)
        .args(["config", "edit"])
        .output()
        .unwrap();

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(!path.exists());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
}

/// A settings file other accounts can read is reported once per process, left as it is by a
/// save that changes nothing, and made owner-only by the next real save.
#[cfg(unix)]
#[test]
fn a_shared_settings_file_is_reported_once_and_made_private_by_the_next_save() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, WORK).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    let mode = || fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    let rename = |label: &str| {
        cli(dir.path())
            .arg("--config")
            .arg(&path)
            .args(["sources", "rename", "gitlab/work", label])
            .output()
            .unwrap()
    };
    let warning = "config.toml can be read by other accounts; Activity Log makes it private the next time it saves it";

    let unchanged = rename("Work");
    assert!(unchanged.status.success(), "{}", stderr(&unchanged));
    assert_eq!(
        stderr(&unchanged).matches(warning).count(),
        1,
        "{}",
        stderr(&unchanged)
    );
    assert_eq!(mode(), 0o644);
    assert_eq!(fs::read_to_string(&path).unwrap(), WORK);

    let renamed = rename("Company");
    assert!(renamed.status.success(), "{}", stderr(&renamed));
    assert_eq!(mode(), 0o600);
    assert!(fs::read_to_string(&path).unwrap().contains("Company"));
}

/// A `.env` in the working folder sets nothing: the CLI resolves its config without the
/// `ACTIVITY_LOG_CONFIG` it names.
#[test]
fn a_dotenv_file_in_the_working_folder_is_not_read() {
    let dir = tempfile::tempdir().unwrap();
    let planted = dir.path().join("planted.toml");
    fs::write(
        dir.path().join(".env"),
        format!("ACTIVITY_LOG_CONFIG={}\n", planted.display()),
    )
    .unwrap();

    let output = cli(dir.path()).args(["config", "path"]).output().unwrap();

    let printed = String::from_utf8_lossy(&output.stdout);
    assert!(!printed.contains("planted.toml"), "{printed}");
    assert!(
        !stderr(&output).contains("planted.toml"),
        "{}",
        stderr(&output)
    );
}
