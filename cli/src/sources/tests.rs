use std::collections::VecDeque;

use activity_log_core::{
    model::{ConnectionId, ProviderInfo, ProviderStatus, Source},
    provider::{connection::Connection, jira::JIRA_INSECURE, provider_config::ConfigSource},
    settings::ConfigFile,
};

use super::{
    Lines, NO_SOURCES, Prompt, SourcesError, add, ask_values, key, list_lines, resolve_connection,
    value_for,
};

/// Answers questions in order and remembers which were secret.
#[derive(Default)]
struct Script {
    answers: VecDeque<&'static str>,
    secret_questions: Vec<String>,
}

impl Script {
    fn new(answers: &[&'static str]) -> Self {
        Self {
            answers: answers.iter().copied().collect(),
            secret_questions: Vec::new(),
        }
    }
}

impl Prompt for Script {
    fn ask(&mut self, question: &str, secret: bool) -> anyhow::Result<String> {
        if secret {
            self.secret_questions.push(question.to_string());
        }
        Ok(self
            .answers
            .pop_front()
            .unwrap_or_else(|| panic!("unexpected question: {question}"))
            .to_string())
    }
}

fn connections(file: &ConfigFile) -> Vec<Connection> {
    file.connections().connections
}

fn value(file: &ConfigFile, id: &str, key: &str) -> Option<String> {
    let id = id.parse::<ConnectionId>().unwrap();
    connections(file)
        .into_iter()
        .find(|connection| connection.id == id)
        .and_then(|connection| connection.values().get(key))
}

#[test]
fn adds_a_connection_and_asks_for_its_values() {
    let mut file = ConfigFile::parse("config.toml", b"").unwrap();
    let mut script = Script::new(&["not a url", "https://gitlab.com", "", "glpat-x", "me"]);

    let values = ask_values(&Source::Gitlab, &mut script).unwrap();
    let id = add(
        &mut file,
        &Source::Gitlab,
        Some("Open Source"),
        None,
        &values,
    )
    .unwrap();

    assert_eq!(id, ConnectionId::new(Source::Gitlab, "open-source"));
    assert_eq!(
        value(&file, "gitlab/open-source", "GITLAB_URL").as_deref(),
        Some("https://gitlab.com")
    );
    assert_eq!(
        value(&file, "gitlab/open-source", "GITLAB_TOKEN").as_deref(),
        Some("glpat-x")
    );
    // The empty answer to the required token is asked again.
    assert_eq!(
        script.secret_questions,
        [
            "token (password, required): ",
            "token (password, required): "
        ]
    );
}

#[test]
fn sets_lists_and_single_values_and_unsets_them() {
    let mut file = ConfigFile::parse("config.toml", b"[[git]]\n\n[[jira]]\n").unwrap();
    let git = ConnectionId::new(Source::Git, "this-computer");
    let jira = ConnectionId::new(Source::Jira, "jira");
    let mut script = Script::new(&["s3cret"]);

    let (dirs, raw) = value_for(
        &git,
        "dirs",
        vec!["~/a".into(), "~/b,~/c".into()],
        &mut script,
    )
    .unwrap();
    file.set(&git, dirs, Some(&raw)).unwrap();
    let (token, raw) = value_for(&jira, "token", Vec::new(), &mut script).unwrap();
    file.set(&jira, token, Some(&raw)).unwrap();

    assert_eq!(
        value(&file, "git/this-computer", "GIT_DIRS").as_deref(),
        Some(r#"["~/a","~/b","~/c"]"#)
    );
    assert_eq!(
        value(&file, "jira/jira", "JIRA_TOKEN").as_deref(),
        Some("s3cret")
    );
    assert_eq!(script.secret_questions, ["token for jira/jira: "]);

    file.set(&git, key(&Source::Git, "dirs").unwrap(), None)
        .unwrap();
    assert_eq!(value(&file, "git/this-computer", "GIT_DIRS"), None);

    let refused = |field: &str, value: &str, script: &mut Script| {
        value_for(&jira, field, vec![value.into()], script)
            .unwrap_err()
            .downcast::<SourcesError>()
            .unwrap()
    };
    assert_eq!(
        refused("insecure", "maybe", &mut script),
        SourcesError::InvalidValue(JIRA_INSECURE)
    );
    let unknown = refused("nope", "x", &mut script);
    assert_eq!(
        unknown,
        SourcesError::UnknownSetting {
            source: Source::Jira,
            field: "nope".to_string(),
        }
    );
    // What the terminal says lists the fields there are.
    assert!(
        unknown
            .to_string()
            .ends_with("use one of: url, token, insecure")
    );
}

/// B42: piped input that ends before a required answer stops the setup instead of asking
/// again forever.
#[test]
fn input_that_ends_early_stops_the_questions() {
    let mut piped = Lines(&b"https://gitlab.com\n"[..]);

    let err = ask_values(&Source::Gitlab, &mut piped).unwrap_err();

    assert_eq!(
        err.to_string(),
        "The input ended before every question was answered; nothing was saved"
    );
    assert!(Lines(&b""[..]).ask("url: ", false).is_err());
    assert_eq!(Lines(&b"x\r\n"[..]).ask("url: ", false).unwrap(), "x");
}

#[test]
fn resolves_connections_by_id_or_unique_name() {
    let file = ConfigFile::parse(
        "config.toml",
        b"[[gitlab]]\nlabel = \"Work\"\n\n[[gitlab]]\nname = \"oss\"\nlabel = \"OSS\"\n\n[[jira]]\nlabel = \"Work\"\n",
    )
    .unwrap();
    let connections = connections(&file);

    assert_eq!(
        resolve_connection(&connections, "oss").unwrap(),
        ConnectionId::new(Source::Gitlab, "oss")
    );
    assert_eq!(
        resolve_connection(&connections, "gitlab/work").unwrap(),
        ConnectionId::new(Source::Gitlab, "work")
    );
    assert!(matches!(
        resolve_connection(&connections, "work"),
        Err(SourcesError::Ambiguous { .. })
    ));
    assert_eq!(
        resolve_connection(&connections, "gitlab/nope"),
        Err(SourcesError::NoConnection("gitlab/nope".to_string()))
    );
}

#[test]
fn lists_connections_in_columns() {
    let info = |source: Source, name: &str, label: &str, usable: bool, reason: Option<&str>| {
        ProviderInfo {
            connection: ConnectionId::new(source, name),
            label: label.to_string(),
            wsl_distro: None,
            status: if usable {
                ProviderStatus::Available
            } else {
                ProviderStatus::Unavailable
            },
            usable,
            supported_os: Vec::new(),
            reason: reason.map(str::to_string),
            problem: None,
            recheckable: false,
            checked_at: String::new(),
        }
    };

    let lines = list_lines(&[
        info(Source::Git, "this-computer", "This computer", true, None),
        info(
            Source::Gitlab,
            "oss",
            "OSS",
            false,
            Some("GitLab credentials invalid"),
        ),
    ]);

    assert_eq!(
        lines,
        [
            "git/this-computer  This computer  ✓ ready",
            "gitlab/oss         OSS            ✗ GitLab credentials invalid",
        ]
    );
    assert_eq!(list_lines(&[]), [NO_SOURCES]);
}
