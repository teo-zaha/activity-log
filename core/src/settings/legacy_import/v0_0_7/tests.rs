use serde_json::json;

use super::{
    LegacyValues, StoreRefusal, encode_config_list, parse_bool, parse_config_list, parse_wsl_repo,
    read_store_provider_config,
};

/// 0.0.7's store reader: each type as it took it, `null` an explicit unset.
#[test]
fn the_store_reader_takes_what_0_0_7_took() {
    let raw = json!({
        "GITLAB_URL": "https://gitlab.example.test",
        "JIRA_INSECURE": true,
        "MAX": 42,
        "GIT_DIRS": ["a", " b ", ""],
        "GITLAB_USER": null,
    });
    let read = read_store_provider_config(Some(&raw)).unwrap();
    assert_eq!(
        read.get("GITLAB_URL"),
        Some(&Some("https://gitlab.example.test".to_string()))
    );
    assert_eq!(read.get("JIRA_INSECURE"), Some(&Some("true".to_string())));
    assert_eq!(read.get("MAX"), Some(&Some("42".to_string())));
    assert_eq!(
        read.get("GIT_DIRS"),
        Some(&Some(encode_config_list(&[
            "a".to_string(),
            " b ".to_string(),
            String::new()
        ])))
    );
    assert_eq!(read.get("GITLAB_USER"), Some(&None));
    assert!(read_store_provider_config(None).unwrap().is_empty());
}

/// Any other type, or a list with an item that is no string, refuses the whole file.
#[test]
fn the_store_reader_refuses_the_file_as_0_0_7_did() {
    let object = json!({ "GITLAB_URL": "x", "JIRA_URL": { "a": 1 } });
    assert_eq!(
        read_store_provider_config(Some(&object)),
        Err(StoreRefusal::UnsupportedType {
            key: "JIRA_URL".to_string()
        })
    );
    let mixed = json!({ "GIT_DIRS": ["a", 1] });
    assert_eq!(
        read_store_provider_config(Some(&mixed)),
        Err(StoreRefusal::NotAListOfStrings {
            key: "GIT_DIRS".to_string()
        })
    );
    assert_eq!(
        read_store_provider_config(Some(&json!(["not", "an", "object"]))),
        Err(StoreRefusal::NotAnObject)
    );
}

#[test]
fn lists_bools_and_blanks_read_as_0_0_7_read_them() {
    assert_eq!(parse_config_list("[]"), Vec::<String>::new());
    assert_eq!(parse_config_list(r#"["a"," b ",""]"#), ["a", "b"]);
    assert_eq!(parse_config_list("x, y"), ["x", "y"]);
    assert_eq!(parse_config_list(r#""C:\repo", d"#), [r"C:\repo", "d"]);
    assert_eq!(parse_bool(" TRUE "), Some(true));
    assert_eq!(parse_bool("yes"), None);

    let values = LegacyValues::new(
        [
            ("GIT_DIRS", Some("[]")),
            ("WSL_DISTROS", Some("  ")),
            ("JIRA_INSECURE", Some("yes")),
            ("GITLAB_URL", Some("")),
            ("GITLAB_USER", None),
            ("OTHER", Some("x")),
        ]
        .into_iter()
        .map(|(key, value)| (key.to_string(), value.map(str::to_string)))
        .collect(),
    );
    assert_eq!(values.list("GIT_DIRS"), Some(Vec::new()));
    assert_eq!(values.list("WSL_DISTROS"), None);
    assert!(!values.bool("JIRA_INSECURE"));
    assert_eq!(values.get("GITLAB_URL"), None);
    assert_eq!(values.get("GITLAB_USER"), None);
    assert!(values.sets_any());
    assert_eq!(values.unknown_keys(), ["OTHER"]);
}

/// Only a key 0.0.7 read, set by these rules, counts as setting something.
#[test]
fn a_file_with_only_unset_or_unknown_keys_sets_nothing() {
    let values = LegacyValues::new(
        [
            ("GITLAB_URL", Some(" ")),
            ("GITLAB_USER", None),
            ("OTHER", Some("x")),
        ]
        .into_iter()
        .map(|(key, value)| (key.to_string(), value.map(str::to_string)))
        .collect(),
    );
    assert!(!values.sets_any());
}

#[test]
fn a_wsl_repository_is_a_distribution_and_an_absolute_path() {
    let repo = parse_wsl_repo("Ubuntu:/home/u/repo/").unwrap();
    assert_eq!(repo.distro, "Ubuntu");
    assert_eq!(repo.path, "/home/u/repo");
    assert_eq!(parse_wsl_repo(r"C:/repo"), None);
    assert_eq!(parse_wsl_repo("Ubuntu:relative"), None);
    assert_eq!(parse_wsl_repo("no-colon"), None);
}
