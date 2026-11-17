use super::ConfigFile;
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
    ConfigFile::parse("config.toml", text).unwrap()
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

#[test]
fn rejects_wsl_outside_git_and_a_kind_that_is_not_a_list() {
    assert!(file("[[gitlab]]\nwsl = \"NixOS\"\n").connections().is_err());
    assert!(file("gitlab = \"x\"\n").connections().is_err());
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
fn saves_where_it_was_loaded_from() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested").join("config.toml");
    let mut config = ConfigFile::load(&path).unwrap();
    config
        .add(&Source::Gitlab, Some("Work"), ConnectionLocation::Local)
        .unwrap();
    config.save().unwrap();

    let reloaded = ConfigFile::load(&path).unwrap();
    assert_eq!(find(&connections(&reloaded), "gitlab/work").label, "Work");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
