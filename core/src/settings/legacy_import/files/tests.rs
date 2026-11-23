use std::fs;

use super::{EnvFound, Read, Unreadable, find_env, read_env, read_settings, referring_keys};
use crate::model::LegacySkipReason;
use crate::settings::legacy_import::v0_0_7::{
    LegacyValues, StoreRefusal, encode_config_list, read_store_provider_config,
};

fn found(env: EnvFound) -> Option<std::path::PathBuf> {
    match env {
        EnvFound::Found { path, .. } => Some(path),
        other => panic!("{other:?}"),
    }
}

fn values(read: Read) -> LegacyValues {
    match read {
        Read::Values { values, .. } => values,
        Read::Unreadable(why) => panic!("{why:?}"),
    }
}

/// `settings.json` is read through 0.0.7's own reader: the expected values are its output for
/// the same `providerConfig`.
#[test]
fn settings_json_reads_as_0_0_7s_store_reader() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    let provider_config = serde_json::json!({
        "GITLAB_URL": "https://gitlab.example.test",
        "GITLAB_TOKEN": "glpat-test-0000",
        "JIRA_INSECURE": true,
        "WSL_DISTROS": "true",
        "MAX": 42,
        "GIT_DIRS": [],
        "WSL_GIT_DIRS": ["a", " b ", ""],
        "JIRA_URL": "x, y",
        "GITLAB_USER": null,
        "JIRA_TOKEN": "",
        "UNKNOWN": "z",
    });
    let store = serde_json::json!({ "providerConfig": provider_config, "theme": "dark" });
    fs::write(&path, serde_json::to_vec_pretty(&store).unwrap()).unwrap();

    let read = values(read_settings(&path).unwrap());
    let expected = LegacyValues::new(read_store_provider_config(Some(&provider_config)).unwrap());
    assert_eq!(read, expected);
    assert_eq!(read.list("GIT_DIRS"), Some(Vec::new()));
    assert!(read.bool("JIRA_INSECURE"));
}

#[test]
fn a_settings_json_0_0_7_refused_gives_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    fs::write(
        &path,
        r#"{ "providerConfig": { "GITLAB_URL": { "nested": true } } }"#,
    )
    .unwrap();
    assert_eq!(
        read_settings(&path),
        Some(Read::Unreadable(Unreadable::Refused(
            StoreRefusal::UnsupportedType {
                key: "GITLAB_URL".to_string()
            }
        )))
    );
    fs::write(&path, "not json").unwrap();
    assert_eq!(
        read_settings(&path),
        Some(Read::Unreadable(Unreadable::NotJson))
    );
    assert_eq!(read_settings(&dir.path().join("missing.json")), None);
}

/// A `.env` as `dotenvy` reads it: comments, quotes, `export`, CRLF, an empty value; the first
/// of a repeated key wins, as 0.0.7's `dotenv()` kept it.
#[test]
fn a_dotenv_file_reads_as_0_0_7_loaded_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(".env");
    fs::write(
        &path,
        "# a comment\r\nexport GITLAB_URL=\"https://gitlab.example.test\"\r\nGITLAB_TOKEN='glpat-test-0000'\r\nGIT_DIRS=\r\nWSL_DISTROS=[]\r\nGITLAB_URL=https://second.example.test\r\n",
    )
    .unwrap();
    let read = values(read_env(&fs::read(&path).unwrap()));
    assert_eq!(read.get("GITLAB_URL"), Some("https://gitlab.example.test"));
    assert_eq!(read.get("GITLAB_TOKEN"), Some("glpat-test-0000"));
    assert_eq!(read.list("GIT_DIRS"), None);
    assert_eq!(read.list("WSL_DISTROS"), Some(Vec::new()));
}

/// A line that does not parse ends the file there, as it ended 0.0.7's load.
#[test]
fn a_dotenv_line_that_does_not_parse_ends_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(".env");
    fs::write(
        &path,
        "GITLAB_URL=https://gitlab.example.test\nnot a pair\nGITLAB_USER=me\n",
    )
    .unwrap();
    let Read::Values { values, stopped } = read_env(&fs::read(&path).unwrap()) else {
        panic!("unreadable");
    };
    assert!(stopped);
    assert_eq!(
        values.get("GITLAB_URL"),
        Some("https://gitlab.example.test")
    );
    assert_eq!(values.get("GITLAB_USER"), None);
}

/// The walk takes the first `.env` up from the start, as 0.0.7's finder did.
#[test]
fn the_first_dotenv_up_the_tree_is_found() {
    let dir = tempfile::tempdir().unwrap();
    let deep = dir.path().join("a").join("b");
    fs::create_dir_all(&deep).unwrap();
    fs::write(dir.path().join(".env"), "GITLAB_USER=outer\n").unwrap();
    assert_eq!(found(find_env(&deep)), Some(dir.path().join(".env")));
    fs::write(dir.path().join("a").join(".env"), "GITLAB_USER=inner\n").unwrap();
    assert_eq!(
        found(find_env(&deep)),
        Some(dir.path().join("a").join(".env"))
    );
    // A folder named `.env` is no file.
    fs::create_dir(deep.join(".env")).unwrap();
    assert_eq!(
        found(find_env(&deep)),
        Some(dir.path().join("a").join(".env"))
    );
}

/// A `.env` another account owns (here through a link to one of root's files) is not read,
/// and the walk stops there.
#[cfg(unix)]
#[test]
fn a_dotenv_another_account_owns_is_not_read() {
    // Root owns every file.
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let deep = dir.path().join("a");
    fs::create_dir_all(&deep).unwrap();
    fs::write(dir.path().join(".env"), "GITLAB_USER=mine\n").unwrap();
    std::os::unix::fs::symlink("/etc/hosts", deep.join(".env")).unwrap();
    assert_eq!(find_env(&deep), EnvFound::NotOwned(deep.join(".env")));
}

#[test]
fn a_list_reads_back_as_written() {
    let items = ["a".to_string(), "b c".to_string()];
    assert_eq!(
        super::super::v0_0_7::parse_config_list(&encode_config_list(&items)),
        items
    );
}

/// The bytes found are the file's own, read through the handle its owner was judged on.
#[test]
fn the_dotenv_found_carries_its_bytes() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join(".env"), "GITLAB_USER=me\n").unwrap();
    assert_eq!(
        find_env(dir.path()),
        EnvFound::Found {
            path: dir.path().join(".env"),
            bytes: b"GITLAB_USER=me\n".to_vec()
        }
    );
}

/// A line that refers to a variable is left out, whatever the environment held: the import
/// never takes a value from it. One that names a `$` only in single quotes or escaped is kept.
#[test]
fn a_dotenv_setting_that_refers_to_a_variable_is_left_out() {
    // SAFETY: a name only this test uses.
    unsafe { std::env::set_var("ACTIVITY_LOG_TEST_LEAKED", "glpat-from-environment") };
    let text = "GITLAB_URL=http://collector.example.test\nGITLAB_TOKEN=${ACTIVITY_LOG_TEST_LEAKED}\nJIRA_TOKEN=\"a$ACTIVITY_LOG_TEST_LEAKED\"\nGITLAB_USER='lit$eral'\nJIRA_URL=https://jira.example.test/\\$x\n";
    let read = values(read_env(text.as_bytes()));
    assert_eq!(read.get("GITLAB_TOKEN"), None);
    assert_eq!(
        read.withheld("GITLAB_TOKEN"),
        Some(LegacySkipReason::ReadsEnvironment)
    );
    assert_eq!(
        read.withheld("JIRA_TOKEN"),
        Some(LegacySkipReason::ReadsEnvironment)
    );
    assert_eq!(read.get("GITLAB_USER"), Some("lit$eral"));
    assert_eq!(read.withheld("JIRA_URL"), None);
    assert_eq!(
        read.get("GITLAB_URL"),
        Some("http://collector.example.test")
    );
    assert!(read.sets_any());
}

#[test]
fn a_reference_is_found_in_every_quoting_it_can_take() {
    let keys = |text: &str| referring_keys(text).into_iter().collect::<Vec<_>>();
    assert_eq!(keys("A=$X\nB=${X}\nexport C=\"$X\"\n"), ["A", "B", "C"]);
    assert!(keys("A='$X'\nB=\\$X\nC=\"\\$X\"\n# D=$X\n").is_empty());
    // A double-quoted value goes on over lines; a single quote inside it quotes nothing.
    assert_eq!(keys("A=\"one\ntwo $X\"\nB=\"it's $X\"\n"), ["A", "B"]);
    // A `$` in a trailing comment counts: better left out than missed.
    assert_eq!(keys("A=plain # costs $5\n"), ["A"]);
}
