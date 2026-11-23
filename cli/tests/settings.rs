//! The CLI as another process writing the settings: the config lock between processes, and
//! the behaviour without a settings location. Every run uses a config in a temporary folder,
//! never the user's.

use std::{
    fs::{self, File},
    path::Path,
    process::{Command, Output, Stdio},
};

/// The CLI run in `dir`, without the `ACTIVITY_LOG_CONFIG` of the environment the tests run in,
/// its OS folders inside `dir`, so no run reaches the user's own settings (nor imports 0.0.7's
/// there). Each test passes its own config or clears the environment.
fn cli(dir: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_activity-log"));
    let home = dir.join("home");
    command
        .env_remove("ACTIVITY_LOG_CONFIG")
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_DATA_HOME", home.join(".local").join("share"))
        .env("APPDATA", home.join("AppData").join("Roaming"))
        .env("LOCALAPPDATA", home.join("AppData").join("Local"))
        .current_dir(dir);
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

/// `sources …` with the key file at `key_file` and `input` on standard input.
fn sources_with_input(dir: &Path, key_file: &Path, args: &[&str], input: Option<&[u8]>) -> Output {
    use std::io::Write as _;

    let mut child = cli(dir)
        .env("ACTIVITY_LOG_SECRET_KEY_FILE", key_file)
        .arg("--config")
        .arg(dir.join("config.toml"))
        .arg("sources")
        .args(args)
        .stdin(match input {
            Some(_) => Stdio::piped(),
            None => Stdio::null(),
        })
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(input) = input {
        child.stdin.take().unwrap().write_all(input).unwrap();
    }
    child.wait_with_output().unwrap()
}

const WORK_AT: &str =
    "[[gitlab]]\nname = \"work\"\nlabel = \"Work\"\nurl = \"https://gitlab.com\"\n";

/// A token given on standard input is written sealed with the key file's key, which the CLI
/// creates, and never as text.
#[test]
fn a_token_from_standard_input_is_written_sealed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let key_file = dir.path().join("secret.key");
    fs::write(&path, WORK_AT).unwrap();

    let output = sources_with_input(
        dir.path(),
        &key_file,
        &["set", "gitlab/work", "token", "-"],
        Some(b"glpat-from-stdin\n"),
    );
    assert!(output.status.success(), "{}", stderr(&output));

    let written = fs::read_to_string(&path).unwrap();
    assert!(!written.contains("glpat-from-stdin"), "{written}");
    assert!(
        written.contains("token = { sealed = \"v1:file:"),
        "{written}"
    );
    assert!(key_file.exists());
}

/// `-` with nothing on standard input (it ended, or brought a blank line) fails and leaves
/// the stored token as it was: only `sources unset` clears it.
#[test]
fn no_token_on_standard_input_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let key_file = dir.path().join("secret.key");
    let text = format!("{WORK_AT}token = \"glpat-stored\"\n");
    fs::write(&path, &text).unwrap();

    for input in [None, Some(&b"\n"[..]), Some(&b"  \t \n"[..])] {
        let output = sources_with_input(
            dir.path(),
            &key_file,
            &["set", "gitlab/work", "token", "-"],
            input,
        );
        assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
        assert_eq!(fs::read_to_string(&path).unwrap(), text);
    }
}

/// A token for a connection without an address is refused, never written as plain text.
#[test]
fn a_token_without_an_address_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let key_file = dir.path().join("secret.key");
    let text = "[[gitlab]]\nname = \"new\"\nlabel = \"New\"\n";
    fs::write(&path, text).unwrap();

    let output = sources_with_input(
        dir.path(),
        &key_file,
        &["set", "gitlab/new", "token", "-"],
        Some(b"glpat-from-stdin\n"),
    );
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert_eq!(fs::read_to_string(&path).unwrap(), text);
}

/// Outside a terminal a new address under a sealed token is refused without the token, and
/// saved with it in the same call (`--secret -`).
#[test]
fn a_new_address_takes_the_token_in_the_same_call() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let key_file = dir.path().join("secret.key");
    fs::write(&path, WORK_AT).unwrap();
    let sealed = sources_with_input(
        dir.path(),
        &key_file,
        &["set", "gitlab/work", "token", "-"],
        Some(b"glpat-first\n"),
    );
    assert!(sealed.status.success(), "{}", stderr(&sealed));
    let before = fs::read_to_string(&path).unwrap();
    let new_url = ["set", "gitlab/work", "url", "https://gitlab.example.test"];

    let refused = sources_with_input(dir.path(), &key_file, &new_url, None);
    assert_eq!(refused.status.code(), Some(1), "{}", stderr(&refused));
    assert_eq!(fs::read_to_string(&path).unwrap(), before);

    let with_token = [&new_url[..], &["--secret", "-"]].concat();
    let saved = sources_with_input(dir.path(), &key_file, &with_token, Some(b"glpat-second\n"));
    assert!(saved.status.success(), "{}", stderr(&saved));
    let written = fs::read_to_string(&path).unwrap();
    assert!(written.contains("https://gitlab.example.test"), "{written}");
    assert!(!written.contains("glpat-second"), "{written}");
    assert_ne!(written, before);
}

/// On the default config path a first command imports 0.0.7's `.env` once: the token sealed
/// with the key file's key, the marker beside, the `.env` as it was; `config path` imports
/// nothing, and a second run nothing again.
#[test]
fn a_first_command_imports_0_0_7s_dotenv_once() {
    let dir = tempfile::tempdir().unwrap();
    let key_file = dir.path().join("secret.key");
    let env = "GITLAB_URL=https://gitlab.example.test\nGITLAB_TOKEN=glpat-test-0000\n";
    fs::write(dir.path().join(".env"), env).unwrap();
    let run = |args: &[&str]| {
        cli(dir.path())
            .env("ACTIVITY_LOG_SECRET_KEY_FILE", &key_file)
            .env_remove("ACTIVITY_LOG_PROFILE")
            .args(args)
            .stdin(Stdio::null())
            .output()
            .unwrap()
    };

    let path = run(&["config", "path"]);
    assert!(path.status.success(), "{}", stderr(&path));
    let config = std::path::PathBuf::from(String::from_utf8_lossy(&path.stdout).trim());
    assert!(!config.exists());

    let listed = run(&["sources"]);
    assert!(listed.status.success(), "{}", stderr(&listed));
    assert!(stderr(&listed).contains("Imported"), "{}", stderr(&listed));
    let written = fs::read_to_string(&config).unwrap();
    assert!(!written.contains("glpat-test-0000"), "{written}");
    assert!(
        written.contains("token = { sealed = \"v1:file:"),
        "{written}"
    );
    assert!(config.with_file_name("legacy-import.toml").is_file());
    assert_eq!(fs::read_to_string(dir.path().join(".env")).unwrap(), env);

    let again = run(&["sources"]);
    assert!(!stderr(&again).contains("Imported"), "{}", stderr(&again));
    assert_eq!(fs::read_to_string(&config).unwrap(), written);
}

/// While 0.0.7's import waits (here no key can be kept: the key file's folder is open to
/// every account), a command that would write `config.toml` is refused, so the import is not
/// given up by accident; one that only reads runs. `config skip-import` gives it up on purpose.
#[cfg(unix)]
#[test]
fn a_waiting_import_holds_back_commands_that_write_the_settings() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let open = dir.path().join("open");
    fs::create_dir(&open).unwrap();
    fs::set_permissions(&open, fs::Permissions::from_mode(0o777)).unwrap();
    fs::write(
        dir.path().join(".env"),
        "GITLAB_URL=https://gitlab.example.test\nGITLAB_TOKEN=glpat-test-0000\n",
    )
    .unwrap();
    let run = |args: &[&str]| {
        cli(dir.path())
            .env("ACTIVITY_LOG_SECRET_KEY_FILE", open.join("secret.key"))
            .env_remove("ACTIVITY_LOG_PROFILE")
            .args(args)
            .stdin(Stdio::null())
            .output()
            .unwrap()
    };
    let path = run(&["config", "path"]);
    let config = std::path::PathBuf::from(String::from_utf8_lossy(&path.stdout).trim());

    let refused = run(&["sources", "add", "system"]);
    assert_eq!(refused.status.code(), Some(1), "{}", stderr(&refused));
    assert!(
        stderr(&refused).contains("skip-import"),
        "{}",
        stderr(&refused)
    );
    assert!(!config.exists());

    let listed = run(&["sources"]);
    assert!(listed.status.success(), "{}", stderr(&listed));
    assert!(!config.exists());

    let skipped = run(&["config", "skip-import"]);
    assert!(skipped.status.success(), "{}", stderr(&skipped));
    assert!(config.with_file_name("legacy-import.toml").is_file());
    let added = run(&["sources", "add", "system"]);
    assert!(added.status.success(), "{}", stderr(&added));
    assert!(!fs::read_to_string(&config).unwrap().contains("gitlab"));
}
