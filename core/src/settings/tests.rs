use std::path::Path;

use super::{ConfigError, ConfigFile};
use crate::{
    model::{ConnectionId, Source},
    provider::{
        connection::{Connection, ConnectionLocation},
        git::GIT_DIRS,
        gitlab::{GITLAB_TOKEN, GITLAB_URL},
        jira::JIRA_INSECURE,
        provider_config::ConfigSource,
    },
};

fn file(text: &str) -> ConfigFile {
    ConfigFile::parse("config.toml", text.as_bytes()).unwrap()
}

fn connections(file: &ConfigFile) -> Vec<Connection> {
    file.connections().unwrap().connections
}

fn ids(connections: &[Connection], source: Source) -> Vec<String> {
    connections
        .iter()
        .filter(|connection| *connection.source() == source)
        .map(|connection| connection.id.name.clone())
        .collect()
}

fn find<'a>(connections: &'a [Connection], id: &str) -> &'a Connection {
    let id = id.parse::<ConnectionId>().unwrap();
    connections
        .iter()
        .find(|connection| connection.id == id)
        .unwrap_or_else(|| panic!("no connection {id}"))
}

#[test]
fn an_empty_file_has_no_sources() {
    assert!(connections(&file("")).is_empty());
    assert!(connections(&file("gitlab = []\n")).is_empty());
}

#[test]
fn reads_several_connections_of_a_kind_with_their_values() {
    let resolved = connections(&file(
        r#"
[[gitlab]]
label = "Work"
url = "https://gitlab.example.com"
token = "work-token"

[[gitlab]]
label = "Open Source"
url = "https://gitlab.com"

[[gitlab]]
name = "mine"
label = "Mine"
"#,
    ));

    assert_eq!(
        ids(&resolved, Source::Gitlab),
        ["work", "open-source", "mine"]
    );
    let work = find(&resolved, "gitlab/work");
    assert_eq!(work.label, "Work");
    assert_eq!(
        work.values().get("GITLAB_URL").as_deref(),
        Some("https://gitlab.example.com")
    );
    assert_eq!(
        work.values().get("GITLAB_TOKEN").as_deref(),
        Some("work-token")
    );
    assert_eq!(
        find(&resolved, "gitlab/open-source")
            .values()
            .get("GITLAB_TOKEN"),
        None
    );
    assert!(ids(&resolved, Source::Jira).is_empty());
}

#[test]
fn reads_git_locations_and_typed_values() {
    let resolved = connections(&file(
        r#"
[[git]]
dirs = ["~/Projects", "/srv/repos"]

[[git]]
wsl = "NixOS"

[[jira]]
insecure = true
"#,
    ));

    let laptop = find(&resolved, "git/this-computer");
    assert_eq!(laptop.label, "This computer");
    assert_eq!(laptop.location, ConnectionLocation::Local);
    assert_eq!(
        laptop.values().get("GIT_DIRS").as_deref(),
        Some(r#"["~/Projects","/srv/repos"]"#)
    );
    let nixos = find(&resolved, "git/wsl-nixos");
    assert_eq!(nixos.label, "WSL · NixOS");
    assert_eq!(nixos.location, ConnectionLocation::Wsl("NixOS".to_string()));
    assert_eq!(
        find(&resolved, "jira/jira")
            .values()
            .get("JIRA_INSECURE")
            .as_deref(),
        Some("true")
    );
}

#[test]
fn reports_unknown_fields_without_failing() {
    let resolved = file("[[gitlab]]\nlabel = \"Work\"\ntokn = \"x\"\n")
        .connections()
        .unwrap();

    assert_eq!(resolved.connections.len(), 1);
    assert_eq!(resolved.warnings, ["gitlab/work: unknown setting `tokn`"]);
}

/// B9: one table the app cannot read no longer takes every connection down.
#[test]
fn skips_wsl_outside_git_and_a_kind_that_is_not_a_list() {
    let resolved = file("gitlab = \"x\"\n\n[[jira]]\nwsl = \"NixOS\"\n\n[[git]]\n")
        .connections()
        .unwrap();

    assert_eq!(ids(&resolved.connections, Source::Git), ["this-computer"]);
    assert!(ids(&resolved.connections, Source::Jira).is_empty());
    assert_eq!(
        resolved.warnings,
        [
            "`gitlab` must be a list of [[gitlab]] tables; it is skipped",
            "jira: only [[git]] connections can set `wsl`; connection 1 is skipped",
        ]
    );
}

/// B9: a broken table between two good ones is skipped, and an edit of the second good one
/// changes that table, by its place in the file.
#[test]
fn edits_address_tables_by_their_place_in_the_file() {
    let mut config = file(
        r#"[[gitlab]]
label = "First"

[[gitlab]]
label = "Broken"
wsl = "NixOS"

[[gitlab]]
label = "Second"
"#,
    );
    let second = ConnectionId::new(Source::Gitlab, "second");
    assert_eq!(
        ids(&connections(&config), Source::Gitlab),
        ["first", "second"]
    );

    config.rename(&second, "Second renamed").unwrap();
    config
        .set(&second, GITLAB_URL, Some("https://gitlab.com"))
        .unwrap();

    let text = config.to_string();
    let broken = text.find("Broken").unwrap();
    assert!(text[broken..].contains("Second renamed"), "{text}");
    assert!(!text[..broken].contains("Second renamed"), "{text}");
    let resolved = connections(&config);
    assert_eq!(find(&resolved, "gitlab/second").label, "Second renamed");
    assert_eq!(
        find(&resolved, "gitlab/first").values().get("GITLAB_URL"),
        None
    );

    config.remove(&second).unwrap();
    assert!(config.to_string().contains("Broken"));
    assert_eq!(ids(&connections(&config), Source::Gitlab), ["first"]);
}

/// B10: several hand-written tables of a kind that has one connection: the first is read.
#[test]
fn reads_only_the_first_table_of_a_single_connection_kind() {
    let mut config =
        file("[[outlook]]\nlabel = \"Mail\"\n\n[[outlook]]\nlabel = \"Other\"\n\n[[system]]\n");
    let resolved = config.connections().unwrap();

    assert_eq!(ids(&resolved.connections, Source::Outlook), ["mail"]);
    assert_eq!(
        resolved.warnings,
        ["outlook: there can be only one connection; connection 2 is skipped"]
    );
    assert!(
        config
            .add(&Source::Outlook, None, ConnectionLocation::Local)
            .is_err()
    );
    config
        .rename(&ConnectionId::new(Source::Outlook, "mail"), "Work mail")
        .unwrap();
    let text = config.to_string();
    assert!(
        text.contains("Work mail") && text.contains("Other"),
        "{text}"
    );
}

/// B2: a parse error names the place, never the text on the bad line.
#[test]
fn a_parse_error_does_not_contain_the_file_text() {
    let err = ConfigFile::parse(
        "config.toml",
        b"[[gitlab]]\nlabel = \"Work\"\ntoken = \"glpat-SECRET\n",
    )
    .unwrap_err();

    assert_eq!((err.line, err.column), (3, 22));
    let shown = format!("{err} {err:?}");
    assert!(!shown.contains("SECRET"), "{shown}");
    assert!(std::error::Error::source(&err).is_none());
    let wrapped = format!("{:#}", anyhow::Error::from(err.clone()));
    assert!(!wrapped.contains("SECRET"), "{wrapped}");
    assert!(
        err.to_string()
            .starts_with("config.toml is not valid TOML (line 3, column")
    );

    let load = ConfigFile::parse("config.toml", b"token = \"glpat-SECRET\xff\"\n").unwrap_err();
    assert_eq!(
        load,
        ConfigError {
            path: "config.toml".into(),
            line: 1,
            column: 22,
            message: "invalid UTF-8".to_string(),
        }
    );
}

#[test]
fn a_relative_path_is_made_absolute() {
    let path = ConfigFile::resolve_path(Some(Path::new("config.toml"))).unwrap();

    assert!(path.is_absolute());
    assert_eq!(path, std::env::current_dir().unwrap().join("config.toml"));
}

#[test]
fn malformed_and_repeated_instances_are_read_as_absent() {
    let resolved = file(
        r#"[[gitlab]]
label = "A"
instance = "0123456789abcdef"

[[gitlab]]
label = "B"
instance = "0123456789abcdef"

[[jira]]
label = "C"
instance = "NOT-HEX"
"#,
    )
    .connections()
    .unwrap();

    assert_eq!(resolved.connections.len(), 3);
    assert_eq!(
        resolved.warnings,
        [
            "gitlab/b: `instance` repeats another connection's; it gets a new one when Activity Log next saves this connection",
            "jira/c: `instance` is not 16 lowercase hex digits; it gets a new one when Activity Log next saves this connection",
        ]
    );
}

#[test]
fn adds_connections_with_numbered_default_labels() {
    let mut config = file("");
    let first = config
        .add(&Source::Gitlab, None, ConnectionLocation::Local)
        .unwrap();
    let second = config
        .add(&Source::Gitlab, None, ConnectionLocation::Local)
        .unwrap();
    let named = config
        .add(
            &Source::Gitlab,
            Some("Open Source"),
            ConnectionLocation::Local,
        )
        .unwrap();
    let wsl = config
        .add(
            &Source::Git,
            None,
            ConnectionLocation::Wsl("NixOS".to_string()),
        )
        .unwrap();
    config
        .set(&named, GITLAB_URL, Some("https://gitlab.com"))
        .unwrap();

    assert_eq!(first, ConnectionId::new(Source::Gitlab, "gitlab"));
    assert_eq!(second, ConnectionId::new(Source::Gitlab, "gitlab-2"));
    assert_eq!(wsl, ConnectionId::new(Source::Git, "wsl-nixos"));
    let resolved = connections(&config);
    assert_eq!(find(&resolved, "gitlab/gitlab-2").label, "GitLab 2");
    assert_eq!(
        find(&resolved, "gitlab/open-source")
            .values()
            .get("GITLAB_URL")
            .as_deref(),
        Some("https://gitlab.com")
    );
}

#[test]
fn refuses_a_second_single_connection_and_wsl_outside_git() {
    let mut config = file("");
    config
        .add(&Source::Outlook, None, ConnectionLocation::Local)
        .unwrap();

    assert!(
        config
            .add(&Source::Outlook, None, ConnectionLocation::Local)
            .is_err()
    );
    assert!(
        config
            .add(
                &Source::Gitlab,
                None,
                ConnectionLocation::Wsl("NixOS".to_string())
            )
            .is_err()
    );
    assert!(
        config
            .add(&Source::Gitlab, Some("  "), ConnectionLocation::Local)
            .is_err()
    );
}

#[test]
fn removing_the_last_connection_removes_the_kind() {
    let mut config = file("[[system]]\n");
    config
        .remove(&ConnectionId::new(Source::System, "system"))
        .unwrap();

    assert!(connections(&config).is_empty());
    assert!(!config.to_string().contains("system"));
    assert!(
        config
            .remove(&ConnectionId::new(Source::System, "system"))
            .is_err()
    );
}

#[test]
fn edits_keep_comments_and_ids() {
    let mut config = file(
        r#"# my settings
[[gitlab]]
# the work account
label = "Work"
url = "https://gitlab.example.com"
"#,
    );
    let id = ConnectionId::new(Source::Gitlab, "work");
    config.rename(&id, "Company").unwrap();
    config.set(&id, GITLAB_TOKEN, Some("secret")).unwrap();
    config.set(&id, GITLAB_URL, None).unwrap();

    let text = config.to_string();
    assert!(text.contains("# my settings"), "{text}");
    assert!(text.contains("# the work account"), "{text}");
    let resolved = connections(&config);
    let company = find(&resolved, "gitlab/work");
    assert_eq!(company.label, "Company");
    assert_eq!(
        company.values().get("GITLAB_TOKEN").as_deref(),
        Some("secret")
    );
    assert_eq!(company.values().get("GITLAB_URL"), None);
}

#[test]
fn writes_values_with_their_toml_type() {
    let mut config = file("");
    let git = config
        .add(&Source::Git, None, ConnectionLocation::Local)
        .unwrap();
    let jira = config
        .add(&Source::Jira, None, ConnectionLocation::Local)
        .unwrap();
    config
        .set(&git, GIT_DIRS, Some(r#"["~/a","~/b"]"#))
        .unwrap();
    config.set(&jira, JIRA_INSECURE, Some("true")).unwrap();

    let text = config.to_string();
    assert!(text.contains(r#"dirs = ["~/a", "~/b"]"#), "{text}");
    assert!(text.contains("insecure = true"), "{text}");
    assert!(config.set(&git, GITLAB_URL, Some("x")).is_err());
}

#[test]
fn an_explicit_name_and_a_label_with_the_same_slug_get_different_ids() {
    let resolved = connections(&file(
        r#"
[[gitlab]]
label = "Work"

[[gitlab]]
name = "Work"
label = "Work account"
"#,
    ));

    assert_eq!(ids(&resolved, Source::Gitlab), ["work-2", "work"]);
    assert_eq!(find(&resolved, "gitlab/work").label, "Work account");
}

#[test]
fn numbers_repeated_explicit_names_and_reports_them() {
    let resolved = file(
        r#"
[[gitlab]]
name = "work"
label = "First"

[[gitlab]]
name = "Work"
label = "Second"

[[gitlab]]
name = "work-2"
label = "Third"
"#,
    )
    .connections()
    .unwrap();

    assert_eq!(
        ids(&resolved.connections, Source::Gitlab),
        ["work", "work-3", "work-2"]
    );
    assert_eq!(
        resolved.warnings,
        [
            r#"gitlab: more than one connection is named `work`; the one labelled "Second" is read as `work-3`"#
        ]
    );
}

#[test]
fn an_edit_writes_the_numbered_name_of_a_repeated_one() {
    let mut config = file("[[gitlab]]\nname = \"work\"\n\n[[gitlab]]\nname = \"work\"\n");
    let second = "gitlab/work-2".parse::<ConnectionId>().unwrap();
    config.rename(&second, "Second").unwrap();

    let resolved = config.connections().unwrap();
    assert!(resolved.warnings.is_empty());
    assert_eq!(
        ids(&resolved.connections, Source::Gitlab),
        ["work", "work-2"]
    );
    assert_eq!(find(&resolved.connections, "gitlab/work-2").label, "Second");
}
